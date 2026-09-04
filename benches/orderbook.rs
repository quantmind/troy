//! `OrderBook` by depth: what a level 2 book costs per operation as it grows.
//!
//! Nothing here is measured against another crate. The decimal benchmarks do
//! that, and no comparable book ships in the crates `Dec` is compared with, so
//! a second column would be invented rather than measured. These numbers answer
//! a different question: which operations are index arithmetic and stay flat as
//! the book deepens, and which walk or shift the levels and do not.
//!
//! Every group is swept over the same depths, so the curves can be read
//! against each other. A flat line is the claim the design makes; a rising one
//! is the price of keeping the levels in a sorted vector.

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;
use troy::{Dec, OrderBook, OrderBookDiff, PriceAmount, dec};

/// Operations per sample for the constant-time groups. One of them is a few
/// nanoseconds, far under the timer's resolution, so a sample covers a run.
const OPS: usize = 256;

/// Operations per sample for the groups that change the shape of the book.
/// Fewer, because each one has to stay small against the depth it is measured
/// at: a run long enough to double an eight-level book would be measuring a
/// different book by the end of it.
const EDITS: usize = 8;

/// A top-of-book feed, a normal venue depth, and two past what an exchange
/// publishes, far enough out for the memmove to separate from the search.
const DEPTHS: [usize; 4] = [8, 32, 128, 512];

const TICK: Dec = dec!(0.01);
const BEST_BID: Dec = dec!(100);
const BEST_ASK: Dec = dec!(100.01);

/// A book `depth` levels deep on each side, capped at `max_depth`, on a one
/// cent tick around 100.
fn book_capped(depth: usize, max_depth: Option<usize>) -> OrderBook {
    let mut book = OrderBook::new(max_depth);
    for level in 0..depth {
        let offset = TICK * Dec::from(level as i32);
        book.bids.set_price_amount(BEST_BID - offset, dec!(10));
        book.asks.set_price_amount(BEST_ASK + offset, dec!(10));
    }
    book
}

/// A book `depth` levels deep on each side, on a one cent tick around 100.
fn book(depth: usize) -> OrderBook {
    let mut book = OrderBook::new(None);
    for level in 0..depth {
        let offset = TICK * Dec::from(level as i32);
        book.bids.set_price_amount(BEST_BID - offset, dec!(10));
        book.asks.set_price_amount(BEST_ASK + offset, dec!(10));
    }
    book
}

/// A deterministic pseudo-random sequence, seeded exactly as the decimal
/// benchmarks seed theirs.
///
/// The access pattern matters as much as the operation. Walking the levels in
/// order gives the prefetcher a straight run and the binary search the same
/// branch decisions every iteration, which is the best case and not the one a
/// feed produces. Scattering the accesses puts the cache miss back in, which is
/// where the difference between one layout and another actually shows.
///
/// Seeded rather than random: criterion compares a run against the last one,
/// and the published snapshot names a commit, so the pattern has to be the
/// same pattern every time. It is drawn into a vector before the clock starts,
/// so what gets measured is the book and not the generator.
fn scattered(count: usize, span: usize) -> Vec<usize> {
    let mut state = 0x2545_F491_4F6C_DD1D_u64;
    (0..count)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as usize % span
        })
        .collect()
}

/// A price already in the book, `level` steps down from the best bid.
fn bid_at(level: usize) -> Dec {
    BEST_BID - TICK * Dec::from(level as i32)
}

/// A price on no tick the book holds, half a step below an existing level, so
/// setting it inserts rather than replaces. Taken from the top of the book,
/// which is where a busy venue actually adds and where everything below has to
/// move to make room.
fn half_tick_at(level: usize) -> Dec {
    bid_at(level) - dec!(0.005)
}

/// A price below every level, for a level added and taken straight back out.
const SCRATCH: Dec = dec!(1);

/// A clone of `base` whose buffer has already grown.
///
/// Cloning a collection allocates exactly the length it copies, so the first
/// insert into a fresh clone reallocates and moves the whole book. At 512
/// levels that is 16 KB, which buries the shift the benchmark is trying to
/// measure and makes every layout look alike. One insert and removal here, in
/// setup where the clock is not running, leaves the spare capacity a book that
/// has been running for any time at all would already have.
fn grown(base: &OrderBook) -> OrderBook {
    let mut book = base.clone();
    book.bids.set_price_amount(SCRATCH, dec!(1));
    book.bids.set_price_amount(SCRATCH, Dec::ZERO);
    book
}

/// Replacing the amount at a price already held: the commonest message on a
/// level 2 feed, and the one that should not care how deep the book is. The
/// search is a binary one and nothing moves.
fn bench_update(c: &mut Criterion) {
    let mut group = c.benchmark_group("book_update");
    group.throughput(Throughput::Elements(OPS as u64));
    for depth in DEPTHS {
        group.bench_with_input(BenchmarkId::new("Dec", depth), &depth, |b, &depth| {
            let mut book = book(depth);
            let touched: Vec<Dec> = scattered(OPS, depth).into_iter().map(bid_at).collect();
            let amounts: Vec<Dec> = scattered(OPS, 900)
                .into_iter()
                .map(|n| dec!(1) + TICK * Dec::from(n as i32))
                .collect();
            b.iter(|| {
                for (price, amount) in touched.iter().zip(&amounts) {
                    book.bids
                        .set_price_amount(black_box(*price), black_box(*amount));
                }
            })
        });
    }
    group.finish();
}

/// Adding a price the book does not hold, at the top, where every level below
/// it shifts. This is the operation that cannot be flat, and the curve is what
/// a sorted vector costs against the searches it makes cheap.
fn bench_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("book_insert");
    group.throughput(Throughput::Elements(EDITS as u64));
    for depth in DEPTHS {
        group.bench_with_input(BenchmarkId::new("Dec", depth), &depth, |b, &depth| {
            let base = book(depth);
            let positions = scattered(EDITS, depth);
            b.iter_batched_ref(
                || grown(&base),
                |book| {
                    for level in &positions {
                        book.bids
                            .set_price_amount(black_box(half_tick_at(*level)), black_box(dec!(5)));
                    }
                },
                BatchSize::SmallInput,
            )
        });
    }
    group.finish();
}

/// Taking a level out, which a zero amount asks for. The same shift as an
/// insert, in the other direction.
fn bench_remove(c: &mut Criterion) {
    let mut group = c.benchmark_group("book_remove");
    group.throughput(Throughput::Elements(EDITS as u64));
    for depth in DEPTHS {
        group.bench_with_input(BenchmarkId::new("Dec", depth), &depth, |b, &depth| {
            let base = book(depth);
            // distinct levels, so each removal actually removes something
            let mut targets = scattered(EDITS * 4, depth);
            targets.sort_unstable();
            targets.dedup();
            targets.truncate(EDITS);
            b.iter_batched_ref(
                || grown(&base),
                |book| {
                    for level in &targets {
                        book.bids
                            .set_price_amount(black_box(bid_at(*level)), black_box(Dec::ZERO));
                    }
                },
                BatchSize::SmallInput,
            )
        });
    }
    group.finish();
}

/// A batch of level updates arriving together, which is the shape a diff
/// message actually has: a handful of amounts changing on both sides at once,
/// none of them adding or removing a level.
fn bench_apply_diff(c: &mut Criterion) {
    let mut group = c.benchmark_group("book_apply_diff");
    group.throughput(Throughput::Elements((EDITS * 2) as u64));
    for depth in DEPTHS {
        group.bench_with_input(BenchmarkId::new("Dec", depth), &depth, |b, &depth| {
            let mut book = book(depth);
            let diff = OrderBookDiff {
                bids: (0..EDITS)
                    .map(|level| PriceAmount {
                        price: bid_at(level % depth),
                        amount: dec!(7),
                    })
                    .collect(),
                asks: (0..EDITS)
                    .map(|level| PriceAmount {
                        price: BEST_ASK + TICK * Dec::from((level % depth) as i32),
                        amount: dec!(7),
                    })
                    .collect(),
            };
            b.iter(|| book.apply_diff(black_box(&diff)))
        });
    }
    group.finish();
}

/// The prices a quote is built from. Each side keeps its levels best first, so
/// these are the front of a vector and an arithmetic mean of two values: the
/// depth behind them should not show up at all.
fn bench_top(c: &mut Criterion) {
    let mut group = c.benchmark_group("book_top");
    group.throughput(Throughput::Elements(OPS as u64));
    for depth in DEPTHS {
        group.bench_with_input(BenchmarkId::new("Dec", depth), &depth, |b, &depth| {
            let book = book(depth);
            b.iter(|| {
                let mut total = Dec::ZERO;
                for _ in 0..OPS {
                    let book = black_box(&book);
                    if let Some(mid) = book.mid_price() {
                        total += mid;
                    }
                    if let Some(spread) = book.spread() {
                        total += spread;
                    }
                }
                total
            })
        });
    }
    group.finish();
}

/// The best price on each side, on its own.
///
/// This is the hottest read a book has - every quote, every risk check, every
/// spread starts here - and the cheapest, so anything the layout adds to it
/// shows up as a proportion. `book_top` covers it too, but wrapped in a
/// midpoint and a subtraction that dilute whatever the access itself costs.
fn bench_best(c: &mut Criterion) {
    let mut group = c.benchmark_group("book_best");
    group.throughput(Throughput::Elements(OPS as u64));
    for depth in DEPTHS {
        group.bench_with_input(BenchmarkId::new("Dec", depth), &depth, |b, &depth| {
            let book = book(depth);
            b.iter(|| {
                let mut total = Dec::ZERO;
                for _ in 0..OPS {
                    let book = black_box(&book);
                    if let Some(bid) = book.bids.best_price() {
                        total += bid;
                    }
                    if let Some(ask) = book.asks.best_price() {
                        total += ask;
                    }
                }
                total
            })
        });
    }
    group.finish();
}

/// Reaching a level by rank rather than by price: an index into the vector,
/// and the reason the levels are kept sorted in the first place.
fn bench_nth(c: &mut Criterion) {
    let mut group = c.benchmark_group("book_nth");
    group.throughput(Throughput::Elements(OPS as u64));
    for depth in DEPTHS {
        group.bench_with_input(BenchmarkId::new("Dec", depth), &depth, |b, &depth| {
            let book = book(depth);
            let ranks = scattered(OPS, depth);
            b.iter(|| {
                let mut total = Dec::ZERO;
                for rank in &ranks {
                    if let Some(entry) = black_box(&book).bids.at(black_box(*rank)) {
                        total += entry.amount;
                    }
                }
                total
            })
        });
    }
    group.finish();
}

/// Reaching a level by price, which is the binary search rather than the index.
fn bench_find(c: &mut Criterion) {
    let mut group = c.benchmark_group("book_find");
    group.throughput(Throughput::Elements(OPS as u64));
    for depth in DEPTHS {
        group.bench_with_input(BenchmarkId::new("Dec", depth), &depth, |b, &depth| {
            let book = book(depth);
            let wanted: Vec<Dec> = scattered(OPS, depth).into_iter().map(bid_at).collect();
            b.iter(|| {
                let mut total = Dec::ZERO;
                for price in &wanted {
                    if let Some(entry) = black_box(&book).bids.find(black_box(*price)) {
                        total += entry.amount;
                    }
                }
                total
            })
        });
    }
    group.finish();
}

/// The two statistics over a side, which walk every level. One call per sample
/// rather than a run of them, so the number is the whole call and the curve is
/// the walk. These are the only operations on a side that are linear on
/// purpose: they were a running total once, corrected on every `set`, which
/// charged the hot path for a figure only an analytic reads.
fn bench_stats(c: &mut Criterion) {
    let mut group = c.benchmark_group("book_stats");
    group.throughput(Throughput::Elements(1));
    for depth in DEPTHS {
        group.bench_with_input(BenchmarkId::new("Dec", depth), &depth, |b, &depth| {
            let book = book(depth);
            b.iter(|| {
                let book = black_box(&book);
                (book.bids.amount_mean(), book.bids.amount_std_dev())
            })
        });
    }
    group.finish();
}

/// A full capped book taking a new best price and giving it straight back: the
/// level that arrives and cancels, which is what a compact book spends its day
/// doing, and the only path where `max_depth` is actually exercised.
///
/// The cap is what the uncapped groups above leave out. An insert into a full
/// side has to check the index against the cap and trim afterwards, and the
/// level it evicts leaves from the far end while the one arriving goes in at
/// the near one. Nothing here clones, because the pair returns the side to the
/// length it started at.
fn bench_churn(c: &mut Criterion) {
    let mut group = c.benchmark_group("book_churn");
    group.throughput(Throughput::Elements(OPS as u64));
    for depth in DEPTHS {
        group.bench_with_input(BenchmarkId::new("Dec", depth), &depth, |b, &depth| {
            let mut book = book_capped(depth, Some(depth));
            let arriving = BEST_BID + TICK;
            b.iter(|| {
                for _ in 0..OPS {
                    book.bids
                        .set_price_amount(black_box(arriving), black_box(dec!(5)));
                    book.bids
                        .set_price_amount(black_box(arriving), black_box(Dec::ZERO));
                }
            })
        });
    }
    group.finish();
}

fn config() -> Criterion {
    Criterion::default()
        .warm_up_time(Duration::from_millis(200))
        .measurement_time(Duration::from_millis(600))
        .sample_size(20)
}

criterion_group! {
    name = benches;
    config = config();
    targets =
        bench_update,
        bench_insert,
        bench_remove,
        bench_apply_diff,
        bench_churn,
        bench_top,
        bench_best,
        bench_nth,
        bench_find,
        bench_stats,
}
criterion_main!(benches);
