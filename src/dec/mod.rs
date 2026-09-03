mod core;
#[cfg(feature = "rust_decimal")]
mod decimal;
mod format;
mod mul;
mod ops;
mod parse;
mod round;
#[cfg(feature = "serde")]
mod serde;

pub use core::Dec;
pub use parse::ParseDecError;

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
