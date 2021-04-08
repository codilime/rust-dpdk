use criterion::{black_box, criterion_group, criterion_main, Criterion};
use pkt_perf::*;

pub fn verify_corectness(vec: &Vec<u8>) {
    let functions : Vec<(&str, fn(&mut [u8]))> = vec![
        ("nat_pnet", nat_pnet),
        ("nat_etherparse_fast_cursor", nat_etherparse_fast_cursor),
        ("nat_etherparse_fast_slice", nat_etherparse_fast_slice),
        // ("nat_etherparse", nat_etherparse), // this one calculates the ip checksum so it will be different
    ];
    let mut smoltcp_buf = vec.clone();
    nat_smoltcp(&mut smoltcp_buf);
    for v in functions {
        let mut buf = vec.clone();
        v.1(&mut buf);
        println!("verifying {}", v.0);
        assert_eq!(smoltcp_buf, buf);
    }
}

pub fn criterion_benchmark(c: &mut Criterion) {
    let vec = vec![
        7, 8, 9, 10, 11, 12, 1, 2, 3, 4, 5, 6, 8, 0, 69, 0, 0, 36, 0, 0, 64, 0, 20, 17, 227, 117,
        192, 168, 1, 1, 192, 168, 1, 2, 21, 179, 30, 97, 0, 16, 56, 82, 1, 2, 3, 4, 5, 6, 7, 8, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 232, 97,
        62, 0, 1, 0, 0, 0, 64, 157, 62, 0, 1, 0, 0, 0, 64, 96, 222, 213, 1, 0, 0, 0, 192, 96, 62,
        0, 1, 0, 0, 0, 192, 96, 222, 213, 1, 0, 0, 0, 128, 0, 1, 0, 1, 0, 255, 255, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 64, 157,
        62, 0, 1, 0, 0, 0,
    ];

    verify_corectness(&vec);

    let mut buf = vec.clone();
    let mut group = c.benchmark_group("nat");
    buf.clone_from(&vec);
    group.bench_function("nat_smoltcp", |b| {
        b.iter(|| nat_smoltcp(black_box(&mut buf)));
    });
    buf.clone_from(&vec);
    group.bench_function("nat_pnet", |b| {
        b.iter(|| nat_pnet(black_box(&mut buf)));
    });
    buf.clone_from(&vec);
    group.bench_function("nat_etherparse_fast_cursor", |b| {
        b.iter(|| nat_etherparse_fast_cursor(black_box(&mut buf)));
    });
    buf.clone_from(&vec);
    group.bench_function("nat_etherparse_fast_slice", |b| {
        b.iter(|| nat_etherparse_fast_slice(black_box(&mut buf)));
    });
    buf.clone_from(&vec);
    group.bench_function("nat_etherparse", |b| {
        b.iter(|| nat_etherparse(black_box(&mut buf)));
    });
    group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
