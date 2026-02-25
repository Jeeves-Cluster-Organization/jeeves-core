//! Frame codec for the IPC wire protocol.
//!
//! Frame format:
//! ```text
//! ┌──────────┬──────────┬────────────────────────┐
//! │ len (4B) │ type(1B) │   msgpack payload      │
//! │ u32 BE   │ u8       │                        │
//! └──────────┴──────────┴────────────────────────┘
//! ```
//! Length = sizeof(type byte) + sizeof(payload), NOT including the 4-byte prefix.

use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Message type: request from client.
pub const MSG_REQUEST: u8 = 0x01;
/// Message type: response to client.
pub const MSG_RESPONSE: u8 = 0x02;
/// Message type: streaming response chunk.
pub const MSG_STREAM_CHUNK: u8 = 0x03;
/// Message type: end of streaming response.
pub const MSG_STREAM_END: u8 = 0x04;
/// Message type: error response to client.
pub const MSG_ERROR: u8 = 0xFF;

/// Read one frame from the stream.
///
/// Returns `(msg_type, payload_bytes)`. Returns `None` on clean EOF.
/// `max_frame_bytes` caps the maximum accepted payload size.
pub async fn read_frame<R: AsyncReadExt + Unpin>(
    reader: &mut R,
    max_frame_bytes: u32,
) -> std::io::Result<Option<(u8, Vec<u8>)>> {
    // Read 4-byte length prefix
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }

    let frame_len = u32::from_be_bytes(len_buf);
    if frame_len > max_frame_bytes {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Frame too large: {} bytes", frame_len),
        ));
    }
    if frame_len < 1 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Frame too short: missing type byte",
        ));
    }

    // Read type byte + payload
    let mut frame_data = vec![0u8; frame_len as usize];
    reader.read_exact(&mut frame_data).await?;

    let msg_type = frame_data[0];
    let payload = frame_data[1..].to_vec();

    Ok(Some((msg_type, payload)))
}

/// Write one frame to the stream.
pub async fn write_frame<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    msg_type: u8,
    payload: &[u8],
) -> std::io::Result<()> {
    let frame_len = 1u32 + payload.len() as u32; // type byte + payload
    writer.write_all(&frame_len.to_be_bytes()).await?;
    writer.write_all(&[msg_type]).await?;
    writer.write_all(payload).await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::io::Cursor;

    const MAX_FRAME: u32 = 5 * 1024 * 1024; // 5 MB, matches production default

    // Helper: build a raw frame from parts
    fn build_raw_frame(len: u32, body: &[u8]) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&len.to_be_bytes());
        buf.extend_from_slice(body);
        buf
    }

    #[tokio::test]
    async fn test_read_frame_valid() {
        let payload = b"hello";
        let mut buf = Vec::new();
        let frame_len = 1u32 + payload.len() as u32;
        buf.extend_from_slice(&frame_len.to_be_bytes());
        buf.push(MSG_REQUEST);
        buf.extend_from_slice(payload);

        let mut cursor = Cursor::new(buf);
        let result = read_frame(&mut cursor, MAX_FRAME).await.unwrap().unwrap();
        assert_eq!(result.0, MSG_REQUEST);
        assert_eq!(result.1, payload);
    }

    #[tokio::test]
    async fn test_read_frame_empty_payload() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&1u32.to_be_bytes()); // frame_len = 1 (type only)
        buf.push(MSG_RESPONSE);

        let mut cursor = Cursor::new(buf);
        let result = read_frame(&mut cursor, MAX_FRAME).await.unwrap().unwrap();
        assert_eq!(result.0, MSG_RESPONSE);
        assert!(result.1.is_empty());
    }

    #[tokio::test]
    async fn test_read_frame_clean_eof() {
        let mut cursor = Cursor::new(Vec::<u8>::new());
        let result = read_frame(&mut cursor, MAX_FRAME).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_read_frame_oversized_rejected() {
        let buf = build_raw_frame(MAX_FRAME + 1, &[MSG_REQUEST]);
        let mut cursor = Cursor::new(buf);
        let err = read_frame(&mut cursor, MAX_FRAME).await.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("too large"));
    }

    #[tokio::test]
    async fn test_read_frame_zero_length_rejected() {
        let buf = build_raw_frame(0, &[]);
        let mut cursor = Cursor::new(buf);
        let err = read_frame(&mut cursor, MAX_FRAME).await.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("too short"));
    }

    #[tokio::test]
    async fn test_read_frame_truncated_body() {
        // Declare frame_len=100 but only provide 5 bytes of body
        let mut buf = Vec::new();
        buf.extend_from_slice(&100u32.to_be_bytes());
        buf.extend_from_slice(&[MSG_REQUEST, 1, 2, 3, 4]);
        let mut cursor = Cursor::new(buf);
        let err = read_frame(&mut cursor, MAX_FRAME).await.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::UnexpectedEof);
    }

    #[tokio::test]
    async fn test_write_read_round_trip() {
        let payload = b"round-trip test payload";
        let mut buf = Vec::new();
        write_frame(&mut buf, MSG_REQUEST, payload).await.unwrap();

        let mut cursor = Cursor::new(buf);
        let (msg_type, data) = read_frame(&mut cursor, MAX_FRAME).await.unwrap().unwrap();
        assert_eq!(msg_type, MSG_REQUEST);
        assert_eq!(data, payload);
    }

    #[tokio::test]
    async fn test_write_read_all_message_types() {
        for &msg_type in &[MSG_REQUEST, MSG_RESPONSE, MSG_STREAM_CHUNK, MSG_STREAM_END, MSG_ERROR] {
            let mut buf = Vec::new();
            write_frame(&mut buf, msg_type, b"test").await.unwrap();
            let mut cursor = Cursor::new(buf);
            let (rt, _) = read_frame(&mut cursor, MAX_FRAME).await.unwrap().unwrap();
            assert_eq!(rt, msg_type);
        }
    }

    // Property-based fuzz tests
    proptest! {
        #[test]
        fn fuzz_read_arbitrary_bytes(data in proptest::collection::vec(any::<u8>(), 0..1024)) {
            // read_frame must never panic on arbitrary input
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let mut cursor = Cursor::new(data);
                let _ = read_frame(&mut cursor, MAX_FRAME).await;
            });
        }

        #[test]
        fn fuzz_read_with_valid_length_prefix(
            frame_len in 0u32..=10_000u32,
            body in proptest::collection::vec(any::<u8>(), 0..256)
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let raw = build_raw_frame(frame_len, &body);
                let mut cursor = Cursor::new(raw);
                let _ = read_frame(&mut cursor, MAX_FRAME).await;
            });
        }

        #[test]
        fn fuzz_write_read_round_trip(
            msg_type in any::<u8>(),
            payload in proptest::collection::vec(any::<u8>(), 0..4096)
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let mut buf = Vec::new();
                write_frame(&mut buf, msg_type, &payload).await.unwrap();
                let mut cursor = Cursor::new(buf);
                let (rt, data) = read_frame(&mut cursor, MAX_FRAME).await.unwrap().unwrap();
                assert_eq!(rt, msg_type);
                assert_eq!(data, payload);
            });
        }

        #[test]
        fn fuzz_oversized_length_always_rejected(frame_len in (MAX_FRAME + 1)..=u32::MAX) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let raw = build_raw_frame(frame_len, &[0x01]); // type byte only
                let mut cursor = Cursor::new(raw);
                let result = read_frame(&mut cursor, MAX_FRAME).await;
                assert!(result.is_err());
            });
        }
    }
}
