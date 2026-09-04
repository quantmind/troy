mod core;
#[cfg(feature = "rust_decimal")]
mod decimal;
mod div;
mod format;
mod mul;
mod ops;
mod parse;
mod round;
#[cfg(feature = "serde")]
mod serde;

pub use core::Dec;
pub use parse::ParseDecError;

/// Builds a [`Dec`] from a decimal literal at compile time.
///
/// The tokens are stringified and handed to [`Dec::parse_const`] inside a
/// `const` block, so the value is a constant and a malformed literal fails the
/// build instead of the trade. The literal accepts everything
/// [`FromStr`](std::str::FromStr) does: a sign, an exponent, `_` separators,
/// and more than [`Dec::SCALE`] decimal places, where the excess rounds half
/// away from zero.
///
/// ```
/// use troy::{Dec, dec};
///
/// const TICK: Dec = dec!(0.01);
/// assert_eq!(dec!(104_237.25) + TICK, dec!(104_237.26));
/// assert_eq!(dec!(-1.5e3), dec!(-1500));
/// assert_eq!(dec!(0.0000000000000000005), Dec::EPSILON);
/// ```
///
/// A literal that does not parse, or one outside the finite range, is rejected
/// by the compiler:
///
/// ```compile_fail
/// # use troy::dec;
/// let price = dec!(banana);
/// ```
#[macro_export]
macro_rules! dec {
    ($($token:tt)+) => {
        const {
            match $crate::Dec::parse_const(stringify!($($token)+)) {
                Some(value) => value,
                None => panic!("invalid decimal literal"),
            }
        }
    };
}
