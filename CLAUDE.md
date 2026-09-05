This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Goal

Troy provides superfast primitives and data structures for High Frequency Trading, where every nanosecond counts. Correctness and performance are the top priorities. The crate is written in safe Rust (`unsafe` is forbidden) and holds two things: `Dec`, a fixed-scale decimal for fast exact arithmetic, and `OrderBook`, a level 2 book built on it.

## Build & Test

```bash
cargo build            # build the library
cargo test --all-features  # run tests with all features enabled
```

Three layers of tests, all run by `cargo test --all-features`:

- unit tests beside the code they cover, in `src/`
- `tests/oracle.rs`, checking multiplication and division against an exact
  256-bit reference that shares no code with them
- `tests/fuzz.rs`, proptest properties over the parser, the renderer, rounding,
  the `f64` conversions and the operator contracts

`make rs-oracle` and `make rs-fuzz` re-run the last two in release over a far
wider sweep than a debug build affords; CI runs both on every PR. A failing
property records its case in `tests/fuzz.proptest-regressions`, which is
committed so the case is replayed from then on.

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

## Releasing

Tagging is the whole release: `.github/workflows/release.yml` fires on a `v*`
tag, checks the tag against `Cargo.toml`, lints, tests, builds the docs, pulls
the matching section out of `docs/release-notes.md`, publishes to crates.io and
opens the GitHub Release with those notes as the body. Everything that can fail
runs before the publish, which cannot be taken back for a version.

So a release is four steps, in this order:

1. Bump `version` in `Cargo.toml`.
2. Add a `## v<version>` section to `docs/release-notes.md`. The tag must match
   the version and the section must exist and be non-empty, or the workflow
   fails — after tagging, before publishing.
3. Commit and push to `main`, and let `build.yml` pass. It runs the full oracle
   sweep and the property sweep in release, which the release workflow does not.
4. `make release` — tags `v<version>` from `Cargo.toml` and pushes the tag.

Never commit during a release. Do steps 1 and 2, then stop and show the release
notes for approval: they are published verbatim as the GitHub Release body and
cannot be revised for a version once it is out. Committing is step 3, and it is
Luca's to run.

### Release notes

`docs/release-notes.md` is the source of truth, one `## v<version>` section per
tagged release, newest first. Keep them compact: grouped bullets under `Fixed`,
`Added`, `Changed` and `Removed`, one bullet per change, one or two lines each.
Say what changed and what a caller does about it. Reasoning belongs in the code,
beside the thing it explains.

Before tagging, `make bench-save` on the reference machine if the numbers moved:
`docs/bench-data.json` carries the commit it was measured at, and the published
page is rendered from it, not from CI.

## Benchmarks

```bash
make bench             # run the criterion benchmarks
make bench-save        # run them and record docs/bench-data.json
make bench-table       # print that snapshot as a terminal table
make bench-report      # open criterion's own report (violin, PDF, sweeps)
```

criterion's report lives in `target/criterion` and is ~30 MB of SVG, so it
stays a local tool. The published site carries the curated summary, the digit
sweeps and the depth sweeps, all rendered from the snapshot.

`docs/bench-data.json` is a committed snapshot measured on a known machine,
carrying the CPU, rustc version and commit it came from. `.dev/bench-report`
reads criterion's JSON and writes that snapshot, and prints the terminal table
from it. CI never measures: a shared runner varies by more than the differences
the page reports.

## Site

`site/` is an Astro project, published by `.github/workflows/pages.yml` to
<https://troy.quantmind.com>, from the root. The custom domain is configured in
the repository's Pages settings, not in a committed CNAME, so `site` in
`site/astro.config.mjs` is the only place it is written down. The project reads
`docs/bench-data.json` and `docs/*.md` directly, so the crate keeps one copy of
each.

```bash
make site              # build into site/dist
make site-dev          # serve locally with hot reload
```

Components are React rendered at build time. Nothing carries a `client:`
directive, so no React reaches the browser: the only script on a page is the
theme toggle and the bar tooltip.

- `site/src/lib/bench.ts` is the data layer. It types the snapshot and holds
  the presentation metadata: which operations appear, in what order, with what
  caption. Adding a benchmark group to a page means adding a line there.
- `site/src/components/` holds the charts. `Sweep` draws a curve against a
  parameter on a linear or log axis, `BarCard` one operation across the
  implementations, `SummaryTable` and `DepthTable` the same numbers as tables.
- `site/src/pages/` holds one file per page, and `site/src/lib/nav.ts` the top
  nav. A new page is a file in each.
- `docs/*.md` is published through `site/src/content.config.ts`. Those files
  are also included into the crate docs with `include_str!`, so they are
  written for rustdoc; `site/src/lib/rustdoc.mjs` strips the leading heading
  and rewrites `crate::` intra-doc links on the way to the web. To publish a
  new one, add it to the loader's pattern and to `DOCS` in `nav.ts`.

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
