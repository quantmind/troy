//! `Dec` against `rust_decimal`, `fastnum` and native `f64`, over the
//! operations a trading system runs on a hot path.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
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
    group.throughput(Throughput::Elements(COUNT as u64));

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

    let mut group = c.benchmark_group("mul");
    group.throughput(Throughput::Elements(COUNT as u64));
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
    group.throughput(Throughput::Elements(COUNT as u64));
    group.bench_function(BenchmarkId::new("f64", COUNT), |b| {
        b.iter(|| {
            for (price, size) in black_box(&prices.floats).iter().zip(&sizes.floats) {
                black_box(price / size);
            }
        })
    });
    group.bench_function(BenchmarkId::new("Dec", COUNT), |b| {
        b.iter(|| {
            for (price, size) in black_box(&prices.decs).iter().zip(&sizes.decs) {
                black_box(*price / *size);
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
    group.throughput(Throughput::Elements(COUNT as u64));

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
    group.throughput(Throughput::Elements(COUNT as u64));

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
    group.throughput(Throughput::Elements(COUNT as u64));
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
    group.throughput(Throughput::Elements(COUNT as u64));

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
    group.throughput(Throughput::Elements(COUNT as u64));

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

// plots stay on: criterion renders the per-parameter line charts that the
// digit-width groups below exist to produce, and the violin comparisons for
// the fixed-width ones
fn config() -> Criterion {
    Criterion::default()
        .warm_up_time(Duration::from_millis(200))
        .measurement_time(Duration::from_millis(600))
        .sample_size(20)
}

fn bench_round_dp(c: &mut Criterion) {
    // the sizes column carries 6 decimals, so rounding to 2 is real work for a
    // scale-carrying type; the prices column has exactly 2 and would be a no-op
    let (_, prices) = columns();
    let mut group = c.benchmark_group("round_dp");
    group.throughput(Throughput::Elements(COUNT as u64));

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
    group.throughput(Throughput::Elements(COUNT as u64));

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
    group.throughput(Throughput::Elements(COUNT as u64));

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
    group.throughput(Throughput::Elements(COUNT as u64));

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

// the digit widths the parse groups sweep. The parser accumulates in a u64 and
// promotes to u128 once a mantissa passes 19 digits, so the interesting shape
// is either side of that crossing rather than a handful of round numbers.
const WIDTHS: [u32; 8] = [1, 4, 8, 12, 18, 19, 22, 26];

// `digits` significant digits with the point in the middle, which is the shape
// a price or a size arrives in rather than a bare integer
fn digit_samples(digits: u32) -> Vec<String> {
    let mut state = 0x2545_F491_4F6C_DD1D_u64;
    let integer_digits = digits.div_ceil(2);
    let fraction_digits = digits - integer_digits;
    (0..COUNT)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let mut text = String::with_capacity(digits as usize + 1);
            let mut next = state;
            for position in 0..digits {
                if position == integer_digits && fraction_digits > 0 {
                    text.push('.');
                }
                next = next.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                let draw = (next >> 60) as u8;
                // never a leading zero, so every sample really carries `digits`
                let digit = match position {
                    0 => draw % 9 + 1,
                    _ => draw % 10,
                };
                text.push((b'0' + digit) as char);
            }
            text
        })
        .collect()
}

fn bench_parse_digits(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_digits");
    group.throughput(Throughput::Elements(COUNT as u64));

    for digits in WIDTHS {
        let texts = digit_samples(digits);
        group.bench_function(BenchmarkId::new("f64", digits), |b| {
            b.iter(|| {
                let mut total = 0.0;
                for text in black_box(&texts) {
                    total += f64::from_str(text).unwrap_or_default();
                }
                total
            })
        });
        group.bench_function(BenchmarkId::new("Dec", digits), |b| {
            b.iter(|| {
                let mut total = Dec::ZERO;
                for text in black_box(&texts) {
                    total += Dec::from_str(text).unwrap_or_default();
                }
                total
            })
        });
        group.bench_function(BenchmarkId::new("rust_decimal", digits), |b| {
            b.iter(|| {
                let mut total = Decimal::ZERO;
                for text in black_box(&texts) {
                    total += Decimal::from_str(text).unwrap_or_default();
                }
                total
            })
        });
        group.bench_function(BenchmarkId::new("fastnum", digits), |b| {
            b.iter(|| {
                let mut total = D128::ZERO;
                for text in black_box(&texts) {
                    total += D128::from_str(text, Context::default()).unwrap_or(D128::ZERO);
                }
                total
            })
        });
    }
    group.finish();
}

fn bench_format_digits(c: &mut Criterion) {
    let mut group = c.benchmark_group("format_digits");
    group.throughput(Throughput::Elements(COUNT as u64));

    for digits in WIDTHS {
        let texts = digit_samples(digits);
        let decs: Vec<Dec> = texts.iter().filter_map(|t| t.parse().ok()).collect();
        let decimals: Vec<Decimal> = texts.iter().filter_map(|t| t.parse().ok()).collect();
        let fastnums: Vec<D128> = texts
            .iter()
            .filter_map(|t| D128::from_str(t, Context::default()).ok())
            .collect();

        group.bench_function(BenchmarkId::new("Dec", digits), |b| {
            b.iter(|| {
                let mut length = 0;
                for value in black_box(&decs) {
                    length += value.to_string().len();
                }
                length
            })
        });
        group.bench_function(BenchmarkId::new("rust_decimal", digits), |b| {
            b.iter(|| {
                let mut length = 0;
                for value in black_box(&decimals) {
                    length += value.to_string().len();
                }
                length
            })
        });
        group.bench_function(BenchmarkId::new("fastnum", digits), |b| {
            b.iter(|| {
                let mut length = 0;
                for value in black_box(&fastnums) {
                    length += value.to_string().len();
                }
                length
            })
        });
    }
    group.finish();
}

// Ingesting a book or a trade feed means filling a vector, not touching one
// value: this pays for the allocation and for every byte the element occupies,
// which is where a 16-byte decimal separates from a wider one.
fn bench_collect(c: &mut Criterion) {
    let (prices, _) = columns();
    let mut group = c.benchmark_group("collect");
    group.throughput(Throughput::Elements(COUNT as u64));

    group.bench_function(BenchmarkId::new("f64", COUNT), |b| {
        b.iter(|| {
            black_box(&prices.texts)
                .iter()
                .filter_map(|text| f64::from_str(text).ok())
                .collect::<Vec<_>>()
        })
    });
    group.bench_function(BenchmarkId::new("Dec", COUNT), |b| {
        b.iter(|| {
            black_box(&prices.texts)
                .iter()
                .filter_map(|text| Dec::from_str(text).ok())
                .collect::<Vec<_>>()
        })
    });
    group.bench_function(BenchmarkId::new("rust_decimal", COUNT), |b| {
        b.iter(|| {
            black_box(&prices.texts)
                .iter()
                .filter_map(|text| Decimal::from_str(text).ok())
                .collect::<Vec<_>>()
        })
    });
    group.bench_function(BenchmarkId::new("fastnum", COUNT), |b| {
        b.iter(|| {
            black_box(&prices.texts)
                .iter()
                .filter_map(|text| D128::from_str(text, Context::default()).ok())
                .collect::<Vec<_>>()
        })
    });
    group.finish();
}

// the same vector without the parsing, so the allocation and the copy are the
// whole measurement and the element width is the only variable left
fn bench_clone(c: &mut Criterion) {
    let (prices, _) = columns();
    let mut group = c.benchmark_group("clone");
    group.throughput(Throughput::Elements(COUNT as u64));

    group.bench_function(BenchmarkId::new("f64", COUNT), |b| {
        b.iter(|| black_box(&prices.floats).clone())
    });
    group.bench_function(BenchmarkId::new("Dec", COUNT), |b| {
        b.iter(|| black_box(&prices.decs).clone())
    });
    group.bench_function(BenchmarkId::new("rust_decimal", COUNT), |b| {
        b.iter(|| black_box(&prices.decimals).clone())
    });
    group.bench_function(BenchmarkId::new("fastnum", COUNT), |b| {
        b.iter(|| black_box(&prices.fastnums).clone())
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
        bench_parse_digits,
        bench_format_digits,
        bench_collect,
        bench_clone,
}
criterion_main!(benches);
