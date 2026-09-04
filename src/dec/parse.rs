use super::core::{Dec, MAX_RAW, POW10};
use std::fmt;
use std::str::FromStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Why a decimal failed to parse.
pub enum ParseDecError {
    /// No digits in the text.
    Empty,
    /// A character that cannot appear in a decimal.
    InvalidDigit,
    /// The value lies outside the finite range.
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
    // set once the mantissa can take no more digits. Text can carry more
    // significant digits than a u128 holds while naming a value well inside the
    // range - one written with forty decimal places, or the exact expansion of
    // a double - so a full mantissa stops the accumulation rather than failing
    // the parse. Every digit after it is below the last one kept, so it can only
    // round that one and then move the exponent.
    let mut saturated = false;
    let mut first_dropped: u8 = 0;
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
                        None => {
                            saturated = true;
                            first_dropped = digit;
                            break;
                        }
                    },
                    None => {
                        saturated = true;
                        first_dropped = digit;
                        break;
                    }
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

    // The mantissa filled, which takes 39 significant digits and so never
    // happens to ordinary text. The remainder is read by a second loop rather
    // than by a test inside the first: the digits above are the hot path, and a
    // branch per digit to ask a question answered the same way every time costs
    // more than repeating the punctuation handling here does. Nothing from here
    // reaches the mantissa. A digit before the point multiplies the value by
    // ten, which the exponent carries; one after it is below the last digit
    // kept and only `first_dropped` remembers it, for the rounding below.
    if saturated {
        while index < len {
            let digit = bytes[index].wrapping_sub(b'0');
            if digit < 10 {
                digits += 1;
                if !seen_point {
                    exponent += 1;
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
        // a dropped digit sits immediately below the last one the mantissa
        // kept, so here, where nothing else rounds, it rounds that one, half
        // away from zero as everywhere else. Below it needs no handling: the
        // scale division rounds a tie away from zero already, and a dropped
        // tail is worth less than one unit of the mantissa, so it can push a
        // remainder no further than from a tie to just past one.
        let magnitude = match first_dropped >= 5 {
            true => match magnitude.checked_add(1) {
                Some(value) => value,
                // the mantissa was already the widest a u128 holds, so this is
                // past the range whichever way the last digit went
                None => return Err(ParseDecError::Overflow),
            },
            false => magnitude,
        };
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

    #[test]
    fn test_precision_past_the_mantissa_rounds_rather_than_failing() {
        // one, spelled with 43 significant digits. The mantissa fills long
        // before the last of them, and every digit it cannot take is below the
        // scale, so the value is one however many ways it is written
        let one = format!("1.{}1", "0".repeat(41));
        assert_eq!(Dec::from_str(&one).unwrap(), Dec::ONE);

        // the widest value the type holds, with one digit of excess either side
        // of the half that decides which way it rounds
        assert_eq!(
            Dec::from_str(&format!("{}1", Dec::MAX)).unwrap(),
            Dec::MAX,
            "a trailing 1 rounds down and leaves MAX where it was"
        );
        assert_eq!(
            Dec::from_str(&format!("{}1", Dec::MIN)).unwrap(),
            Dec::MIN,
            "and the same at the other end of a symmetric range"
        );
        assert_eq!(
            Dec::from_str(&format!("{}5", Dec::MAX)),
            Err(ParseDecError::Overflow),
            "a trailing 5 rounds up, which really is past the range"
        );
    }

    #[test]
    fn test_the_exact_expansion_of_a_double_parses() {
        // what Python's `Decimal(0.1)` prints: the exact binary value of the
        // double nearest 0.1, 55 significant digits naming something that sits
        // comfortably inside the range. A producer that serialises floats this
        // way is the likeliest source of a mantissa wider than a u128.
        assert_eq!(
            Dec::from_str("0.1000000000000000055511151231257827021181583404541015625").unwrap(),
            Dec::from_str("0.100000000000000006").unwrap()
        );
    }

    #[test]
    fn test_a_mantissa_at_the_u128_limit_still_rounds_correctly() {
        // the mantissa here is exactly u128::MAX, so the digit that follows
        // cannot round it in place without overflowing; the scale division
        // carries the decision instead, which is why the two are separated
        assert_eq!(
            Dec::from_str("34028236692093846346.33746074317682114559").unwrap(),
            Dec::from_str("34028236692093846346.337460743176821146").unwrap()
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
