//! Superfast primitives and data structures for high frequency trading.
//!
//! # [`Dec`], the number
//!
//! A fixed-scale decimal backed by an `i128` carrying [`Dec::SCALE`] decimal
//! places. Arithmetic is exact within a finite range of roughly +/-1.7e20;
//! anything that leaves that range becomes [`Dec::NAN`], which propagates
//! through every subsequent operation so the fault reaches the boundary where
//! [`Dec::is_finite`] is checked.
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
//! # [`OrderBook`], the structure
//!
//! A level 2 book built on [`Dec`]. Each side keeps its levels best first, so
//! the best price, the n-th level and the depth cut are index arithmetic
//! rather than searches, and a level enters only if [`PriceAmount::is_valid`]
//! accepts it — a non-finite price would sort against every other level, and a
//! non-finite amount would poison the side's running total for good.
//!
//! ```
//! use troy::{OrderBook, dec};
//!
//! let mut book = OrderBook::new(Some(10));
//! book.bids.set_price_amount(dec!(104_237.25), dec!(3));
//! book.asks.set_price_amount(dec!(104_237.30), dec!(1));
//!
//! assert_eq!(book.mid_price(), Some(dec!(104_237.275)));
//! assert_eq!(book.spread(), Some(dec!(0.05)));
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
mod market;

pub use dec::*;
pub use market::*;

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
