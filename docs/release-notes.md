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
