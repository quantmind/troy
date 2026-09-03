use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use fastnum::D128;
use fastnum::decimal::Context;
use rust_decimal::Decimal;
use rust_decimal::prelude::*;
use std::hint::black_box;
use std::str::FromStr;
use std::time::Duration;
use troy::Dec;

const COUNT: usize = 1_024;

fn samples(integer_range: u64, decimals: u32) -> Vec<String> {
    let mut state = 0x2545_F491_4F6C_DD1D_u64;
    let scale = 10_u64.pow(decimals);
    (0..COUNT)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let integer = state % integer_range + 1;
            let fraction = (state >> 32) % scale;
            format!("{integer}.{fraction:0width$}", width = decimals as usize)
        })
        .collect()
}

struct Column {
    floats: Vec<f64>,
    decs: Vec<Dec>,
    decimals: Vec<Decimal>,
    fastnums: Vec<D128>,
    texts: Vec<String>,
}

impl Column {
    fn new(integer_range: u64, decimals: u32) -> Self {
        let texts = samples(integer_range, decimals);
        Self {
            floats: texts.iter().map(|text| text.parse().unwrap()).collect(),
            decs: texts.iter().map(|text| text.parse().unwrap()).collect(),
            decimals: texts.iter().map(|text| text.parse().unwrap()).collect(),
            fastnums: texts
                .iter()
                .map(|text| D128::from_str(text, Context::default()).unwrap())
                .collect(),
            texts,
        }
    }
}

fn columns() -> (Column, Column) {
    (Column::new(10_000, 2), Column::new(100, 6))
}

fn bench_add(c: &mut Criterion) {
    let (prices, _) = columns();
    let mut group = c.benchmark_group("add");

    group.bench_function(BenchmarkId::new("f64", COUNT), |b| {
        b.iter(|| {
            let mut total = 0.0;
            for value in black_box(&prices.floats) {
                total += *value;
            }
            total
        })
    });
    group.bench_function(BenchmarkId::new("Dec", COUNT), |b| {
        b.iter(|| {
            let mut total = Dec::ZERO;
            for value in black_box(&prices.decs) {
                total += *value;
            }
            total
        })
    });
    group.bench_function(BenchmarkId::new("rust_decimal", COUNT), |b| {
        b.iter(|| {
            let mut total = Decimal::ZERO;
            for value in black_box(&prices.decimals) {
                total += *value;
            }
            total
        })
    });
    group.bench_function(BenchmarkId::new("fastnum", COUNT), |b| {
        b.iter(|| {
            let mut total = D128::ZERO;
            for value in black_box(&prices.fastnums) {
                total += *value;
            }
            total
        })
    });
    group.finish();
}

fn bench_mul_div(c: &mut Criterion) {
    let (prices, sizes) = columns();

    // Dec has no multiply or divide yet; these arms are the target to beat
    let mut group = c.benchmark_group("mul");
    group.bench_function(BenchmarkId::new("f64", COUNT), |b| {
        b.iter(|| {
            for (price, size) in black_box(&prices.floats).iter().zip(&sizes.floats) {
                black_box(price * size);
            }
        })
    });
    group.bench_function(BenchmarkId::new("Dec", COUNT), |b| {
        b.iter(|| {
            for (price, size) in black_box(&prices.decs).iter().zip(&sizes.decs) {
                black_box(*price * *size);
            }
        })
    });
    group.bench_function(BenchmarkId::new("rust_decimal", COUNT), |b| {
        b.iter(|| {
            for (price, size) in black_box(&prices.decimals).iter().zip(&sizes.decimals) {
                black_box(price * size);
            }
        })
    });
    group.bench_function(BenchmarkId::new("fastnum", COUNT), |b| {
        b.iter(|| {
            for (price, size) in black_box(&prices.fastnums).iter().zip(&sizes.fastnums) {
                black_box(*price * *size);
            }
        })
    });
    group.finish();

    let mut group = c.benchmark_group("div");
    group.bench_function(BenchmarkId::new("f64", COUNT), |b| {
        b.iter(|| {
            for (price, size) in black_box(&prices.floats).iter().zip(&sizes.floats) {
                black_box(price / size);
            }
        })
    });
    group.bench_function(BenchmarkId::new("rust_decimal", COUNT), |b| {
        b.iter(|| {
            for (price, size) in black_box(&prices.decimals).iter().zip(&sizes.decimals) {
                black_box(price / size);
            }
        })
    });
    group.bench_function(BenchmarkId::new("fastnum", COUNT), |b| {
        b.iter(|| {
            for (price, size) in black_box(&prices.fastnums).iter().zip(&sizes.fastnums) {
                black_box(*price / *size);
            }
        })
    });
    group.finish();
}

fn bench_cmp(c: &mut Criterion) {
    let (prices, sizes) = columns();
    let mut group = c.benchmark_group("cmp");

    group.bench_function(BenchmarkId::new("f64", COUNT), |b| {
        b.iter(|| {
            black_box(&prices.floats)
                .iter()
                .zip(&sizes.floats)
                .filter(|(left, right)| left < right)
                .count()
        })
    });
    group.bench_function(BenchmarkId::new("Dec", COUNT), |b| {
        b.iter(|| {
            black_box(&prices.decs)
                .iter()
                .zip(&sizes.decs)
                .filter(|(left, right)| left < right)
                .count()
        })
    });
    group.bench_function(BenchmarkId::new("rust_decimal", COUNT), |b| {
        b.iter(|| {
            black_box(&prices.decimals)
                .iter()
                .zip(&sizes.decimals)
                .filter(|(left, right)| left < right)
                .count()
        })
    });
    group.bench_function(BenchmarkId::new("fastnum", COUNT), |b| {
        b.iter(|| {
            black_box(&prices.fastnums)
                .iter()
                .zip(&sizes.fastnums)
                .filter(|(left, right)| left < right)
                .count()
        })
    });
    group.finish();
}

fn bench_f64_boundary(c: &mut Criterion) {
    let (prices, _) = columns();
    let mut group = c.benchmark_group("to_f64");

    group.bench_function(BenchmarkId::new("Dec", COUNT), |b| {
        b.iter(|| {
            for value in black_box(&prices.decs) {
                black_box(value.to_f64());
            }
        })
    });
    group.bench_function(BenchmarkId::new("rust_decimal", COUNT), |b| {
        b.iter(|| {
            for value in black_box(&prices.decimals) {
                black_box(value.to_f64().unwrap_or_default());
            }
        })
    });
    group.bench_function(BenchmarkId::new("fastnum", COUNT), |b| {
        b.iter(|| {
            for value in black_box(&prices.fastnums) {
                black_box(f64::from(*value));
            }
        })
    });
    group.finish();

    let mut group = c.benchmark_group("from_f64");
    group.bench_function(BenchmarkId::new("Dec", COUNT), |b| {
        b.iter(|| {
            for value in black_box(&prices.floats) {
                black_box(Dec::from_f64(*value).unwrap_or_default());
            }
        })
    });
    group.bench_function(BenchmarkId::new("rust_decimal", COUNT), |b| {
        b.iter(|| {
            for value in black_box(&prices.floats) {
                black_box(Decimal::from_f64(*value).unwrap_or_default());
            }
        })
    });
    group.bench_function(BenchmarkId::new("fastnum", COUNT), |b| {
        b.iter(|| {
            for value in black_box(&prices.floats) {
                black_box(D128::from(*value));
            }
        })
    });
    group.finish();
}

fn bench_parse(c: &mut Criterion) {
    let (prices, _) = columns();
    let mut group = c.benchmark_group("parse");

    group.bench_function(BenchmarkId::new("f64", COUNT), |b| {
        b.iter(|| {
            let mut total = 0.0;
            for text in black_box(&prices.texts) {
                total += f64::from_str(text).unwrap_or_default();
            }
            total
        })
    });
    group.bench_function(BenchmarkId::new("Dec", COUNT), |b| {
        b.iter(|| {
            let mut total = Dec::ZERO;
            for text in black_box(&prices.texts) {
                total += Dec::from_str(text).unwrap_or_default();
            }
            total
        })
    });
    group.bench_function(BenchmarkId::new("rust_decimal", COUNT), |b| {
        b.iter(|| {
            let mut total = Decimal::ZERO;
            for text in black_box(&prices.texts) {
                total += Decimal::from_str(text).unwrap_or_default();
            }
            total
        })
    });
    group.bench_function(BenchmarkId::new("fastnum", COUNT), |b| {
        b.iter(|| {
            let mut total = D128::ZERO;
            for text in black_box(&prices.texts) {
                total += D128::from_str(text, Context::default()).unwrap_or(D128::ZERO);
            }
            total
        })
    });
    group.finish();
}

fn bench_format(c: &mut Criterion) {
    let (prices, _) = columns();
    let mut group = c.benchmark_group("format");

    group.bench_function(BenchmarkId::new("f64", COUNT), |b| {
        b.iter(|| {
            let mut length = 0;
            for value in black_box(&prices.floats) {
                length += value.to_string().len();
            }
            length
        })
    });
    group.bench_function(BenchmarkId::new("Dec", COUNT), |b| {
        b.iter(|| {
            let mut length = 0;
            for value in black_box(&prices.decs) {
                length += value.to_string().len();
            }
            length
        })
    });
    group.bench_function(BenchmarkId::new("rust_decimal", COUNT), |b| {
        b.iter(|| {
            let mut length = 0;
            for value in black_box(&prices.decimals) {
                length += value.to_string().len();
            }
            length
        })
    });
    group.bench_function(BenchmarkId::new("fastnum", COUNT), |b| {
        b.iter(|| {
            let mut length = 0;
            for value in black_box(&prices.fastnums) {
                length += value.to_string().len();
            }
            length
        })
    });
    group.finish();
}

fn config() -> Criterion {
    Criterion::default()
        .warm_up_time(Duration::from_millis(200))
        .measurement_time(Duration::from_millis(600))
        .sample_size(20)
        .without_plots()
}

fn bench_round_dp(c: &mut Criterion) {
    // the sizes column carries 6 decimals, so rounding to 2 is real work for a
    // scale-carrying type; the prices column has exactly 2 and would be a no-op
    let (_, prices) = columns();
    let mut group = c.benchmark_group("round_dp");

    // f64 has no scale, so the analogue is a scale-round-unscale round trip
    group.bench_function(BenchmarkId::new("f64", COUNT), |b| {
        b.iter(|| {
            for value in black_box(&prices.floats) {
                black_box((value * 100.0).round() / 100.0);
            }
        })
    });
    group.bench_function(BenchmarkId::new("Dec", COUNT), |b| {
        b.iter(|| {
            for value in black_box(&prices.decs) {
                black_box(value.round_dp(2));
            }
        })
    });
    group.bench_function(BenchmarkId::new("rust_decimal", COUNT), |b| {
        b.iter(|| {
            for value in black_box(&prices.decimals) {
                black_box(value.round_dp(2));
            }
        })
    });
    group.bench_function(BenchmarkId::new("fastnum", COUNT), |b| {
        b.iter(|| {
            for value in black_box(&prices.fastnums) {
                black_box(value.round(2));
            }
        })
    });
    group.finish();
}

fn bench_round_to_step(c: &mut Criterion) {
    // the sizes column carries 6 decimals, so a step of 0.01 is real work
    let (_, prices) = columns();
    let step_f64 = 0.01_f64;
    let step_dec = Dec::from_str("0.01").unwrap();
    let step_decimal = Decimal::from_str("0.01").unwrap();
    let step_fastnum = D128::from_str("0.01", Context::default()).unwrap();
    let mut group = c.benchmark_group("round_to_step");

    group.bench_function(BenchmarkId::new("f64", COUNT), |b| {
        b.iter(|| {
            for value in black_box(&prices.floats) {
                black_box((value / step_f64).round() * step_f64);
            }
        })
    });
    group.bench_function(BenchmarkId::new("Dec", COUNT), |b| {
        b.iter(|| {
            for value in black_box(&prices.decs) {
                black_box(value.round_to_step(step_dec));
            }
        })
    });
    group.bench_function(BenchmarkId::new("rust_decimal", COUNT), |b| {
        b.iter(|| {
            for value in black_box(&prices.decimals) {
                black_box(step_decimal * (value / step_decimal).round_dp(0));
            }
        })
    });
    group.bench_function(BenchmarkId::new("fastnum", COUNT), |b| {
        b.iter(|| {
            for value in black_box(&prices.fastnums) {
                black_box(step_fastnum * (*value / step_fastnum).round(0));
            }
        })
    });
    group.finish();
}

fn bench_floor(c: &mut Criterion) {
    let (prices, _) = columns();
    let mut group = c.benchmark_group("floor");

    group.bench_function(BenchmarkId::new("f64", COUNT), |b| {
        b.iter(|| {
            for value in black_box(&prices.floats) {
                black_box(value.floor());
            }
        })
    });
    group.bench_function(BenchmarkId::new("Dec", COUNT), |b| {
        b.iter(|| {
            for value in black_box(&prices.decs) {
                black_box(value.floor());
            }
        })
    });
    group.bench_function(BenchmarkId::new("rust_decimal", COUNT), |b| {
        b.iter(|| {
            for value in black_box(&prices.decimals) {
                black_box(value.floor());
            }
        })
    });
    group.bench_function(BenchmarkId::new("fastnum", COUNT), |b| {
        b.iter(|| {
            for value in black_box(&prices.fastnums) {
                black_box(value.floor());
            }
        })
    });
    group.finish();
}

fn bench_ceil(c: &mut Criterion) {
    let (prices, _) = columns();
    let mut group = c.benchmark_group("ceil");

    group.bench_function(BenchmarkId::new("f64", COUNT), |b| {
        b.iter(|| {
            for value in black_box(&prices.floats) {
                black_box(value.ceil());
            }
        })
    });
    group.bench_function(BenchmarkId::new("Dec", COUNT), |b| {
        b.iter(|| {
            for value in black_box(&prices.decs) {
                black_box(value.ceil());
            }
        })
    });
    group.bench_function(BenchmarkId::new("rust_decimal", COUNT), |b| {
        b.iter(|| {
            for value in black_box(&prices.decimals) {
                black_box(value.ceil());
            }
        })
    });
    group.bench_function(BenchmarkId::new("fastnum", COUNT), |b| {
        b.iter(|| {
            for value in black_box(&prices.fastnums) {
                black_box(value.ceil());
            }
        })
    });
    group.finish();
}

criterion_group! {
    name = benches;
    config = config();
    targets =
        bench_add,
        bench_cmp,
        bench_mul_div,
        bench_round_dp,
        bench_round_to_step,
        bench_floor,
        bench_ceil,
        bench_f64_boundary,
        bench_parse,
        bench_format,
}
criterion_main!(benches);
