//! Superfast numeric primitives for high frequency trading.
//!
//! The crate is built around [`Dec`], a fixed-scale decimal backed by an
//! `i128` carrying [`Dec::SCALE`] decimal places. Arithmetic is exact within a
//! finite range of roughly +/-1.7e20; anything that leaves that range becomes
//! [`Dec::NAN`], which propagates through every subsequent operation so the
//! fault reaches the boundary where [`Dec::is_finite`] is checked.
//!
//! ```
//! use troy::{Dec, dec};
//!
//! let notional = dec!(104_237.25) * dec!(0.00135);
//! assert_eq!(notional, dec!(140.72028750));
//! assert!(notional.is_finite());
//!
//! // overflow poisons rather than clamping or panicking
//! assert!((Dec::MAX + Dec::ONE).is_nan());
//! ```
//!
//! See the [`design`] module for the reasoning behind the representation, the
//! NaN state, and the ordering.

#![forbid(unsafe_code)]
#![deny(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::todo,
    clippy::unimplemented,
    clippy::unreachable
)]

mod dec;

pub use dec::*;

/// How `Dec` is represented, what the NaN state means, and the rules every
/// operation follows.
#[doc = include_str!("../docs/design.md")]
pub mod design {}

/// How `Dec` is laid out in memory, and how that compares with other decimals.
#[doc = include_str!("../docs/memory-layout.md")]
pub mod memory_layout {}

/// Release notes, one section per tagged release.
#[doc = include_str!("../docs/release-notes.md")]
pub mod release_notes {}
