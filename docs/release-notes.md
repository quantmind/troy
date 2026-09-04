# Release Notes

This page is the source of truth for troy release notes. Each section below
maps to a tagged release on
[GitHub](https://github.com/quantmind/troy/releases). When a new tag is
pushed, the matching section is extracted by
`.github/workflows/release.yml` and published as the GitHub Release body.

Keep entries compact: grouped bullets under `Fixed`, `Added`, `Changed` and
`Removed`, one bullet per change, one or two lines each. Say what changed and
what a caller does about it. Reasoning belongs in the code, beside the thing it
explains.

## v0.1.1

Fixed

- Parser reported `Overflow` for text carrying more significant digits than a
  `u128` holds, even for values well inside the range: `1.0` written with 43
  decimal places, or a float printed as its exact expansion. Surplus digits now
  round the last one kept instead of failing the parse.

Added

- `OrderBook::imbalance` — `(bid - ask) / (bid + ask)` at the touch, in `[-1, 1]`.
- `OrderBook::range_imbalance(from, to)` — the same over a half open band of
  levels. An empty or inverted band reads `None`, not zero.
- `tests/fuzz.rs` — proptest properties over `Dec`; `make rs-fuzz` sweeps wider.
- `tests/fuzz_book.rs` — capped-book invariants at 10, 50, 100 and 200 levels.
- `benches/orderbook.rs` — book operations by depth; `make bench` runs it.

Changed

- `OrderBookSide` drops its running `total_amount`. `amount_mean` and
  `amount_std_dev` sum on demand, `trim` becomes a `truncate`, and a non-finite
  amount can no longer poison a side for good.

## v0.1.0

First release: `Dec`, a fixed-scale decimal backed by an `i128` with
`Dec::SCALE` decimal places, with parsing, formatting, arithmetic, rounding,
and optional `serde` and `rust_decimal` support.

Arithmetic is exact over a symmetric finite range of roughly +/-1.7e20.
Anything leaving it becomes `Dec::NAN`, which propagates through every later
operation so the fault reaches the boundary where `Dec::is_finite` is checked,
rather than being clamped to a plausible number or raised as a panic on a hot
path. The ordering stays total, so `Dec` remains usable as a map key and in a
sorted book. See the `design` module for the reasoning.
