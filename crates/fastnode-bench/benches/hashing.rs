use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use fastnode_util::hash::blake3_bytes;
use std::io::Write;
use tempfile::NamedTempFile;

fn bench_blake3_bytes(c: &mut Criterion) {
    let mut group = c.benchmark_group("blake3_bytes");

    for size in [64, 1024, 16 * 1024, 256 * 1024, 1024 * 1024] {
        let data = vec![0xABu8; size];
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &data, |b, data| {
            b.iter(|| blake3_bytes(black_box(data)));
        });
    }

    group.finish();
}

fn bench_blake3_file(c: &mut Criterion) {
    let mut group = c.benchmark_group("blake3_file");

    for size in [64, 1024, 16 * 1024, 256 * 1024] {
        let data = vec![0xABu8; size];
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(&data).unwrap();
        file.flush().unwrap();

        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &file, |b, file| {
            b.iter(|| fastnode_util::hash::blake3_file(black_box(file.path())));
        });
    }

    group.finish();
}

criterion_group!(benches, bench_blake3_bytes, bench_blake3_file);
criterion_main!(benches);
