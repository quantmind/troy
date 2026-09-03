use super::core::{MAX_RAW, ONE_U};

const ONE_64: u64 = 1_000_000_000_000_000_000;

const CHUNK: u64 = 1_000_000_000;

// 10^9 factors as 2^9 * 5^9. Multiplying by the inverse of the odd part mod
// 2^128 and rotating out the 2^9 divides exactly by 10^9 without a division
// instruction, and the same rotated value is the divisibility test.
// Both constants are checked against their definitions in the tests.
const INV_5_9: u128 = 200_825_005_271_070_938_688_767_579_709_359_692_909;
const LIMIT_1E9: u128 = 340_282_366_920_938_463_463_374_607_431;

// n / 10^9 when 10^9 divides n, None when it does not
#[inline(always)]
pub(crate) fn div_exact_1e9(n: u128) -> Option<u128> {
    let rotated = n.wrapping_mul(INV_5_9).rotate_right(9);
    match rotated <= LIMIT_1E9 {
        true => Some(rotated),
        false => None,
    }
}

// a * b / 10^18. Prices and sizes carry at most 9 decimals, so both raws
// normally divide by 10^9 and the whole product is (a/10^9) * (b/10^9), whose
// factors fit a u64 for any realistic value: one widening multiply, no division.
#[inline(always)]
pub(crate) fn mul_raw(a: i128, b: i128) -> Option<i128> {
    let negative = (a < 0) != (b < 0);
    let (a, b) = (a.unsigned_abs(), b.unsigned_abs());
    match (div_exact_1e9(a), div_exact_1e9(b)) {
        (Some(left), Some(right)) => {
            let magnitude = match (u64::try_from(left), u64::try_from(right)) {
                (Ok(left), Ok(right)) => left as u128 * right as u128,
                _ => left.checked_mul(right)?,
            };
            signed(negative, magnitude)
        }
        _ => mul_wide(a, b, negative),
    }
}

// finer than 9 decimals: split both at the decimal point, which leaves two
// fractions below 10^18 that finish in 64-bit lanes
fn mul_wide(a: u128, b: u128, negative: bool) -> Option<i128> {
    // a NaN operand has magnitude 2^127, which 10^9 does not divide, so it
    // always arrives here rather than on the fast path above. Nothing finite
    // reaches this magnitude, so one bound recognises it for both operands.
    if (a | b) > MAX_RAW as u128 {
        return None;
    }
    let (a_int, a_frac) = (a / ONE_U, (a % ONE_U) as u64);
    let (b_int, b_frac) = (b / ONE_U, (b % ONE_U) as u64);
    let (tail, remainder) = mul_frac(a_frac, b_frac);
    let magnitude = a
        .checked_mul(b_int)?
        .checked_add(a_int.checked_mul(b_frac as u128)?)?
        .checked_add(tail as u128)?;
    let rounded = match remainder * 2 >= ONE_64 {
        true => magnitude.checked_add(1)?,
        false => magnitude,
    };
    signed(negative, rounded)
}

// a * b / 10^18 for two fractions below 10^18; quotient and remainder
fn mul_frac(a: u64, b: u64) -> (u64, u64) {
    let (a_high, a_low) = (a / CHUNK, a % CHUNK);
    let (b_high, b_low) = (b / CHUNK, b % CHUNK);
    let high = a_high * b_high;
    let middle = a_high * b_low + a_low * b_high;
    let tail = (middle % CHUNK) * CHUNK + a_low * b_low;
    (high + middle / CHUNK + tail / ONE_64, tail % ONE_64)
}

#[inline(always)]
fn signed(negative: bool, magnitude: u128) -> Option<i128> {
    // the range is symmetric, so one limit serves both signs
    if magnitude > MAX_RAW as u128 {
        return None;
    }
    Some(match negative {
        true => -(magnitude as i128),
        false => magnitude as i128,
    })
}

#[cfg(test)]
mod multiply {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::Dec;

    use crate::dec;
    use std::str::FromStr;

    #[test]
    fn test_the_magic_constants_match_their_definitions() {
        // INV_5_9 is the inverse of the odd part of 10^9 modulo 2^128
        let five_pow_nine: u128 = 5_u128.pow(9);
        assert_eq!(INV_5_9.wrapping_mul(five_pow_nine), 1);
        assert_eq!(LIMIT_1E9, u128::MAX / 1_000_000_000);
    }

    #[test]
    fn test_exact_division_matches_plain_division() {
        for value in [
            0_u128,
            1_000_000_000,
            123_456_789_000_000_000,
            u128::MAX / 2,
        ] {
            let expected = match value.is_multiple_of(1_000_000_000) {
                true => Some(value / 1_000_000_000),
                false => None,
            };
            assert_eq!(div_exact_1e9(value), expected, "{value}");
        }
        assert_eq!(div_exact_1e9(7), None);
        assert_eq!(div_exact_1e9(999_999_999), None);
    }

    #[test]
    fn test_market_precision_takes_the_fast_path() {
        assert!(div_exact_1e9(dec!(104_237.25).into_raw().unsigned_abs()).is_some());
        assert!(div_exact_1e9(dec!(0.00135).into_raw().unsigned_abs()).is_some());
        assert_eq!(dec!(104_237.25) * dec!(0.00135), dec!(140.72028750));
    }

    #[test]
    fn test_finer_than_nine_decimals_takes_the_wide_path() {
        let fine = Dec::from_str("0.0000000001").unwrap();
        assert!(div_exact_1e9(fine.into_raw().unsigned_abs()).is_none());
        assert_eq!(fine * dec!(3), Dec::from_str("0.0000000003").unwrap());
        assert_eq!(Dec::EPSILON * dec!(1000), Dec::from_raw(1_000));
    }

    #[test]
    fn test_both_paths_agree() {
        for (left, right) in [
            ("1", "1"),
            ("2.5", "4"),
            ("-3.75", "8"),
            ("0.1", "0.1"),
            ("104237.25", "0.00135"),
            ("10000000000", "10000000000"),
            ("0.000000001", "0.000000001"),
        ] {
            let a = Dec::from_str(left).unwrap();
            let b = Dec::from_str(right).unwrap();
            let wide = mul_wide(
                a.into_raw().unsigned_abs(),
                b.into_raw().unsigned_abs(),
                (a.into_raw() < 0) != (b.into_raw() < 0),
            );
            assert_eq!(Some((a * b).into_raw()), wide, "{left} * {right}");
        }
    }

    #[test]
    fn test_signs_identities_and_overflow() {
        assert_eq!(dec!(-2) * dec!(3), dec!(-6));
        assert_eq!(dec!(-2) * dec!(-3), dec!(6));
        assert_eq!(dec!(7.5) * Dec::ONE, dec!(7.5));
        assert_eq!(dec!(7.5) * Dec::ZERO, Dec::ZERO);
        assert_eq!(dec!(7.5) * Dec::NEG_ONE, dec!(-7.5));
        assert_eq!(Dec::MAX.checked_mul(Dec::MAX), None);
        assert_eq!(Dec::MAX.checked_mul(dec!(-2)), None);
        assert_eq!(Dec::MAX.saturating_mul(dec!(-2)), Dec::MIN);

        let mut value = dec!(3);
        value *= dec!(4);
        assert_eq!(value, dec!(12));
    }
}
