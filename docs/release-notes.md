# Release Notes

This page is the source of truth for troy release notes. Each section below
maps to a tagged release on
[GitHub](https://github.com/quantmind/troy/releases). When a new tag is
pushed, the matching section is extracted by
`.github/workflows/release.yml` and published as the GitHub Release body.

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
