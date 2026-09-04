This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Goal

Troy provides superfast primitives and data structures for High Frequency Trading, where every nanosecond counts. Correctness and performance are the top priorities. The crate is written in safe Rust (`unsafe` is forbidden) and holds two things: `Dec`, a fixed-scale decimal for fast exact arithmetic, and `OrderBook`, a level 2 book built on it.

## Build & Test

```bash
cargo build            # build the library
cargo test --all-features  # run tests with all features enabled
```

## Lint & Format

```bash
cargo fmt --check      # check formatting (no changes)
cargo fmt              # apply formatting
cargo clippy -- -Dwarnings  # lint, treating warnings as errors
```

Or use the Make targets:

```bash
make rs-lint           # fmt + clippy (auto-fixes formatting)
make rs-test           # test with all features
```

## Benchmarks

```bash
make bench             # run the criterion benchmarks
make bench-save        # run them and record docs/bench-data.json
make bench-page        # render that snapshot into site/index.html
make bench-report      # open criterion's own report (violin, PDF, sweeps)
```

criterion's report lives in `target/criterion` and is ~30 MB of SVG, so it
stays a local tool. The published page carries the curated summary and the
digit sweeps, both rendered from the snapshot.

`docs/bench-data.json` is a committed snapshot measured on a known machine,
carrying the CPU, rustc version and commit it came from. `.dev/bench-report`
reads criterion's JSON and renders both the terminal table and the page that
`.github/workflows/pages.yml` publishes to GitHub Pages. CI never measures:
a shared runner varies by more than the differences the page reports.

## Alternatives Crates

This crates is design to be fast and efficient for high-frequency trading scenarios, where performance is critical.
It does not aim to be a general-purpose decimal library with extensive features; instead, it focuses on speed and correctness for HFT use cases.
Some alternative crates for decimal arithmetic include:

- [rust_decimal](https://github.com/paupino/rust-decimal) - a widely used general-purpose decimal library
- [fastnum](https://github.com/neogenie/fastnum) - for fast decimal arithmetic
- [fixed](https://gitlab.com/tspiteri/fixed) - a fixed-point arithmetic library for Rust

Some of the design decisions in are based around these crates.
The bechmarks compare this library with `fastnum` and `rust_decimal` as well as the native `f64`.


## Makefile Conventions

- The `help` target must be the first target in the file.
- All other targets must be sorted alphabetically.
- Targets should be separated by one blank line only.
- Each target should have a one-line description, starting with `##`, that describes what the target does. This description is used by the `help` target to generate documentation for all targets.
