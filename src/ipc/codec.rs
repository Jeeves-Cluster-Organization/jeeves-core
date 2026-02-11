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
