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
