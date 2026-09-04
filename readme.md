# Troy

[![build](https://github.com/quantmind/troy/actions/workflows/build.yml/badge.svg)](https://github.com/quantmind/troy/actions/workflows/build.yml)
[![crates.io](https://img.shields.io/crates/v/troy.svg)](https://crates.io/crates/troy)
[![docs.rs](https://img.shields.io/docsrs/troy)](https://docs.rs/troy)

Superfast primitives and data structures for high frequency trading, in safe
Rust, `unsafe` is forbidden crate-wide.

Two things live here: **`Dec`**, a fixed-scale decimal for exact arithmetic,
and **`OrderBook`**, a level 2 book built on it.

```toml
[dependencies]
troy = "0.1"
```

## `Dec`

An `i128` carrying 18 decimal places. Arithmetic is exact within roughly
±1.7e20, and anything that leaves that range becomes `Dec::NAN`.

```rust
use troy::{Dec, dec};

let notional = dec!(104_237.25) * dec!(0.00135);
assert_eq!(notional, dec!(140.72028750));

// overflow poisons rather than clamping or panicking
assert!((Dec::MAX + Dec::ONE).is_nan());
```

NaN propagates through every later operation, so a fault reaches the boundary
where `is_finite` is checked instead of being clamped to a plausible number or
raised as a panic on a hot path. The ordering stays total, so `Dec` works as a
map key and in a sorted book. `checked_*` and `saturating_*` are there for
callers who would rather decide on the spot.

## `OrderBook`

Each side keeps its levels best first — descending for bids, ascending for
asks — so the best price, the n-th level and the depth cut are index
arithmetic rather than searches.

```rust
use troy::{OrderBook, dec};

let mut book = OrderBook::new(Some(10));
book.bids.set_price_amount(dec!(104_237.25), dec!(3));
book.asks.set_price_amount(dec!(104_237.30), dec!(1));

assert_eq!(book.mid_price(), Some(dec!(104_237.275)));
assert_eq!(book.spread(), Some(dec!(0.05)));
```

A level enters only if `PriceAmount::is_valid` accepts it. A non-finite price
would sort against every other level rather than among them, and a non-finite
amount would carry into every statistic taken over the side for as long as the
level stood.

## Benchmarks

**[troy.quantmind.com](https://troy.quantmind.com)** carries the full results:
every operation charted, the parse and format cost by digit width, and the
machine the numbers came from.

The summary — nanoseconds per operation against `rust_decimal`, `fastnum` and
native `f64`, measured on an Intel Core Ultra 9 285H. `f64` is the inexact
reference, not a competitor; **bold** marks the fastest decimal.

| operation | f64 | `Dec` | `rust_decimal` | `fastnum` |
|---|---|---|---|---|
| cmp | 0.10 | **0.30** | 3.21 | 4.15 |
| add | 0.37 | **0.81** | 1.74 | 7.78 |
| mul | 0.15 | **3.13** | 6.04 | 4.22 |
| div | 0.72 | **15.70** | 26.80 | 79.54 |
| round to step | 1.04 | **3.32** | 24.37 | 19.80 |
| parse | 6.57 | **7.38** | 10.81 | 15.12 |
| format | 57.28 | **23.35** | 29.25 | 49.47 |
| to f64 | — | **1.31** | 16.80 | 1.88 |
| from f64 | — | **8.89** | 81.51 | 75.94 |

Run them yourself with `make bench`, or `make bench-save` to record a snapshot
and `make bench-page` to render the page locally. Published numbers come from a
committed snapshot rather than a CI run, because a shared runner varies by more
than the differences being measured.

## Features

| feature | what it adds |
|---|---|
| `serde` | `Serialize`/`Deserialize` for `Dec`, as a decimal string |
| `rust_decimal` | conversions to and from `rust_decimal::Decimal` |
| `testing` | `OrderBookBuilder` and `RandomWalk` for building and simulating books |

All are off by default.

## Documentation

API documentation lives on **[docs.rs/troy](https://docs.rs/troy)**, built with
every feature enabled.

The `design` module covers the reasoning behind the representation, the NaN
state, the ordering, and how multiplication and division reach 187 bits in
`u128` steps. `memory_layout` compares the layout with other decimal crates,
and `release_notes` carries one section per tagged release.

## License

MIT.
