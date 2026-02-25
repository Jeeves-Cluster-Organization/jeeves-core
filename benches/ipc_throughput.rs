//! IPC codec throughput benchmark.
//!
//! Measures read_frame/write_frame round-trip latency and throughput
//! using Criterion.

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use jeeves_core::ipc::codec::{read_frame, write_frame, MSG_REQUEST};
use std::io::Cursor;

const MAX_FRAME: u32 = 5 * 1024 * 1024;

fn bench_write_frame(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let payload_sizes: &[usize] = &[0, 64, 1024, 4096, 65536];

    let mut group = c.benchmark_group("write_frame");
    for &size in payload_sizes {
        let payload = vec![0xABu8; size];
        group.bench_with_input(BenchmarkId::from_parameter(size), &payload, |b, p| {
            b.iter(|| {
                rt.block_on(async {
                    let mut buf = Vec::with_capacity(size + 5);
                    write_frame(&mut buf, MSG_REQUEST, black_box(p)).await.unwrap();
                    buf
                })
            });
        });
    }
    group.finish();
}

fn bench_read_frame(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let payload_sizes: &[usize] = &[0, 64, 1024, 4096, 65536];

    let mut group = c.benchmark_group("read_frame");
    for &size in payload_sizes {
        // Pre-build the wire frame
        let payload = vec![0xABu8; size];
        let wire = rt.block_on(async {
            let mut buf = Vec::new();
            write_frame(&mut buf, MSG_REQUEST, &payload).await.unwrap();
            buf
        });

        group.bench_with_input(BenchmarkId::from_parameter(size), &wire, |b, w| {
            b.iter(|| {
                rt.block_on(async {
                    let mut cursor = Cursor::new(black_box(w.as_slice()));
                    read_frame(&mut cursor, MAX_FRAME).await.unwrap()
                })
            });
        });
    }
    group.finish();
}

fn bench_round_trip(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let payload = vec![0xABu8; 1024];

    c.bench_function("round_trip_1kb", |b| {
        b.iter(|| {
            rt.block_on(async {
                let mut buf = Vec::with_capacity(1029);
                write_frame(&mut buf, MSG_REQUEST, black_box(&payload)).await.unwrap();
                let mut cursor = Cursor::new(buf);
                read_frame(&mut cursor, MAX_FRAME).await.unwrap()
            })
        });
    });
}

criterion_group!(benches, bench_write_frame, bench_read_frame, bench_round_trip);
criterion_main!(benches);
