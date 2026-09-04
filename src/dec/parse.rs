use super::core::{Dec, MAX_RAW, POW10};
use std::fmt;
use std::str::FromStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseDecError {
    Empty,
    InvalidDigit,
    Overflow,
}

impl fmt::Display for ParseDecError {
    #[inline(always)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseDecError::Empty => f.write_str("no digits in decimal"),
            ParseDecError::InvalidDigit => f.write_str("invalid character in decimal"),
            ParseDecError::Overflow => f.write_str("decimal out of range"),
        }
    }
}

impl std::error::Error for ParseDecError {}

pub(crate) const fn parse_bytes(bytes: &[u8]) -> Result<i128, ParseDecError> {
    let len = bytes.len();
    let mut index = 0;
    while index < len && bytes[index] == b' ' {
        index += 1;
    }
    let mut negative = false;
    if index < len && (bytes[index] == b'-' || bytes[index] == b'+') {
        negative = bytes[index] == b'-';
        index += 1;
    }
    while index < len && bytes[index] == b' ' {
        index += 1;
    }

    // digits accumulate in a 64-bit lane and promote to 128 only when a mantissa
    // wider than 19 digits demands it, which keeps ordinary inputs on the fast path
    let mut mantissa: u64 = 0;
    let mut wide: u128 = 0;
    let mut promoted = false;
    let mut exponent: i32 = 0;
    let mut digits = 0;
    let mut seen_point = false;
    while index < len {
        let digit = bytes[index].wrapping_sub(b'0');
        if digit < 10 {
            if promoted {
                // exact rather than a worst-case-digit guard, which would
                // reject mantissas that do fit, Dec::MAX's own text among them
                wide = match wide.checked_mul(10) {
                    Some(value) => match value.checked_add(digit as u128) {
                        Some(value) => value,
                        None => return Err(ParseDecError::Overflow),
                    },
                    None => return Err(ParseDecError::Overflow),
                };
            } else if mantissa > (u64::MAX - 9) / 10 {
                wide = mantissa as u128 * 10 + digit as u128;
                promoted = true;
            } else {
                mantissa = mantissa * 10 + digit as u64;
            }
            digits += 1;
            if seen_point {
                exponent -= 1;
            }
            index += 1;
            continue;
        }
        let byte = bytes[index];
        if byte == b'e' || byte == b'E' {
            break;
        }
        if byte == b'_' {
            index += 1;
            continue;
        }
        if byte != b'.' || seen_point {
            return Err(ParseDecError::InvalidDigit);
        }
        seen_point = true;
        index += 1;
    }
    if digits == 0 {
        return Err(ParseDecError::Empty);
    }

    if index < len {
        index += 1;
        let mut exponent_negative = false;
        if index < len && (bytes[index] == b'-' || bytes[index] == b'+') {
            exponent_negative = bytes[index] == b'-';
            index += 1;
        }
        let mut exponent_digits = 0;
        let mut value: i32 = 0;
        while index < len {
            let byte = bytes[index];
            if byte < b'0' || byte > b'9' {
                return Err(ParseDecError::InvalidDigit);
            }
            if value > 1_000 {
                return Err(ParseDecError::Overflow);
            }
            value = value * 10 + (byte - b'0') as i32;
            exponent_digits += 1;
            index += 1;
        }
        if exponent_digits == 0 {
            return Err(ParseDecError::Empty);
        }
        exponent += if exponent_negative { -value } else { value };
    }

    let magnitude = match promoted {
        true => wide,
        false => mantissa as u128,
    };
    let shift = exponent + Dec::SCALE as i32;
    let scaled = if shift >= 0 {
        if shift > 38 {
            return Err(ParseDecError::Overflow);
        }
        match magnitude.checked_mul(POW10[shift as usize] as u128) {
            Some(value) => value,
            None => return Err(ParseDecError::Overflow),
        }
    } else if -shift > 38 {
        0
    } else {
        div_round_u128(magnitude, POW10[(-shift) as usize] as u128)
    };
    // the range is symmetric, so one limit serves both signs
    if scaled > MAX_RAW as u128 {
        return Err(ParseDecError::Overflow);
    }
    Ok(match negative {
        true => -(scaled as i128),
        false => scaled as i128,
    })
}

const fn div_round_u128(numerator: u128, divisor: u128) -> u128 {
    let quotient = numerator / divisor;
    let remainder = numerator % divisor;
    let half = divisor / 2;
    match remainder > half || (remainder == half && divisor & 1 == 0) {
        true => quotient + 1,
        false => quotient,
    }
}

impl FromStr for Dec {
    type Err = ParseDecError;

    #[inline(always)]
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        parse_bytes(value.as_bytes()).map(Self)
    }
}

#[cfg(test)]
mod parsing {
    #![allow(clippy::unwrap_used)]

    use crate::Dec;
    use crate::ParseDecError;
    use crate::dec;
    use std::str::FromStr;

    #[test]
    fn test_parse_trims_and_rounds_excess_precision() {
        assert_eq!(
            Dec::from_str("1.50").unwrap(),
            Dec::from_str("1.5").unwrap()
        );
        assert_eq!(
            Dec::from_str("0.0000000000000000005").unwrap(),
            Dec::EPSILON,
            "half rounds away from zero at the scale boundary"
        );
        assert_eq!(
            Dec::from_str("0.0000000000000000015").unwrap(),
            Dec::from_raw(2)
        );
    }

    #[test]
    fn test_parse_exponent_form() {
        assert_eq!(
            Dec::from_str("1e-8").unwrap(),
            Dec::from_raw(10_000_000_000)
        );
        assert_eq!(
            Dec::from_str("1.5E3").unwrap(),
            Dec::from_str("1500").unwrap()
        );
        assert_eq!(
            Dec::from_str("-2.5e-1").unwrap(),
            Dec::from_str("-0.25").unwrap()
        );
    }

    #[test]
    fn test_parse_rejects_malformed_input() {
        assert_eq!(Dec::from_str(""), Err(ParseDecError::Empty));
        assert_eq!(Dec::from_str("1.2.3"), Err(ParseDecError::InvalidDigit));
        assert_eq!(Dec::from_str("abc"), Err(ParseDecError::InvalidDigit));
    }

    #[test]
    fn test_macro_is_const() {
        const HALF: Dec = dec!(0.5);
        const NEGATIVE: Dec = dec!(-1.25);
        assert_eq!(HALF, Dec::from_raw(500_000_000_000_000_000));
        assert_eq!(NEGATIVE.to_string(), "-1.25");
    }
}

#[cfg(test)]
mod wide_values {
    #![allow(clippy::unwrap_used)]

    use crate::Dec;
    use crate::ParseDecError;
    use std::str::FromStr;

    #[test]
    fn test_max_round_trips_through_its_own_string() {
        let text = Dec::MAX.to_string();
        assert_eq!(text.chars().filter(char::is_ascii_digit).count(), 39);
        assert_eq!(Dec::from_str(&text).unwrap(), Dec::MAX);
    }

    #[test]
    fn test_mantissas_wider_than_a_u64_parse() {
        // 20 digits, one past what the u64 accumulator alone can hold
        let text = "12345678901234567890";
        assert_eq!(
            Dec::from_str(text).unwrap().into_raw(),
            12_345_678_901_234_567_890_i128 * 1_000_000_000_000_000_000
        );
        assert_eq!(
            Dec::from_str("170141183460469231731.687303715884105727").unwrap(),
            Dec::MAX
        );
    }

    #[test]
    fn test_a_mantissa_past_i128_is_still_rejected() {
        assert_eq!(
            Dec::from_str("1701411834604692317316873037158841057270"),
            Err(ParseDecError::Overflow)
        );
    }
}

#[cfg(test)]
mod errors {
    #![allow(clippy::unwrap_used)]

    use crate::Dec;
    use crate::ParseDecError;
    use std::str::FromStr;

    #[test]
    fn test_const_parsing_reports_failure_without_panicking() {
        assert_eq!(
            Dec::parse_const("1.5"),
            Some(Dec::from_raw(1_500_000_000_000_000_000))
        );
        assert_eq!(
            Dec::parse_const("-0.25"),
            Some(Dec::from_raw(-250_000_000_000_000_000))
        );
        assert_eq!(Dec::parse_const(""), None);
        assert_eq!(Dec::parse_const("1.2.3"), None);
        assert_eq!(Dec::parse_const("banana"), None);
    }

    #[test]
    fn test_from_u64_covers_the_whole_range() {
        assert_eq!(Dec::from_u64(0), Dec::ZERO);
        assert_eq!(Dec::from_u64(1), Dec::ONE);
        assert_eq!(
            Dec::from_u64(u64::MAX),
            Dec::from_str("18446744073709551615").unwrap()
        );
    }

    #[test]
    fn test_error_messages() {
        assert_eq!(ParseDecError::Empty.to_string(), "no digits in decimal");
        assert_eq!(
            ParseDecError::InvalidDigit.to_string(),
            "invalid character in decimal"
        );
        assert_eq!(ParseDecError::Overflow.to_string(), "decimal out of range");
    }

    #[test]
    fn test_parse_error_is_a_std_error() {
        let error: Box<dyn std::error::Error> = Box::new(ParseDecError::Empty);
        assert_eq!(error.to_string(), "no digits in decimal");
    }
}
