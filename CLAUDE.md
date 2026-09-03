This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Goal

Troy provides superfast numeric primitives for High Frequency Trading, where every nanosecond counts. Correctness and performance are the top priorities: the crate is a fixed-scale decimal type for fast, exact arithmetic, written in safe Rust (`unsafe` is forbidden).

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
cargo bench            # run the criterion benchmarks
```

## Alternatives Crates

This crates is design to be fast and efficient for high-frequency trading scenarios, where performance is critical.
It does not aim to be a general-purpose decimal library with extensive features; instead, it focuses on speed and correctness for HFT use cases.
Some alternative crates for decimal arithmetic include:

* [rust_decimal](https://github.com/paupino/rust-decimal) - a widely used general-purpose decimal library
* [fastnum](https://github.com/neogenie/fastnum) - for fast decimal arithmetic
* [fixed](https://gitlab.com/tspiteri/fixed) - a fixed-point arithmetic library for Rust

Some of the design decisions in are based around these crates.
The bechmarks compare this library with `fastnum` and `rust_decimal` as well as the native `f64`.
