use super::core::MAX_RAW;

const CHUNK: u128 = 1_000_000_000;
const ONE_U: u128 = 1_000_000_000_000_000_000;

// r * CHUNK stays inside a u128 only while r is at or below this
const CHUNK_LIMIT: u128 = u128::MAX / CHUNK;

// a / b at scale 10^18, so the raw quotient is a * 10^18 / b. That product
// needs 187 bits, which no u128 holds, so all three paths below reach it by
// steps that each stay inside one: divide first, then scale the remainder.
//
// None on division by zero, on a NaN operand, and when the quotient leaves the
// finite range, which the operators turn into NaN.
#[inline(always)]
pub(crate) fn div_raw(a: i128, b: i128) -> Option<i128> {
    if b == 0 {
        return None;
    }
    let negative = (a < 0) != (b < 0);
    let (a, b) = (a.unsigned_abs(), b.unsigned_abs());
    // a NaN operand has magnitude 2^127, one past the finite range, and
    // nothing finite reaches it, so one bound recognises either side
    if (a | b) > MAX_RAW as u128 {
        return None;
    }

    // a divisor of at most 9 decimal places divides by 10^9 exactly, which
    // cancels half the scaling before it is ever applied; every price, size
    // and tick takes this path
    let magnitude = match b.is_multiple_of(CHUNK) {
        true => scale_by(a, b / CHUNK, CHUNK)?,
        false => wide(a, b)?,
    };
    signed(negative, magnitude)
}

// round(x * factor / divisor), where divisor * factor must not exceed a u128.
// Splitting x into quotient and remainder first keeps the scaling on the
// remainder, which is below the divisor and so has room for the factor.
#[inline(always)]
fn scale_by(x: u128, divisor: u128, factor: u128) -> Option<u128> {
    let (quotient, remainder) = (x / divisor, x % divisor);
    let scaled = remainder * factor;
    let total = quotient
        .checked_mul(factor)?
        .checked_add(scaled / divisor)?;
    round_half_away(total, scaled % divisor, divisor)
}

// the general shape: reduce a below b, then walk the remaining 10^18 up in two
// 10^9 steps, each of which fits because the remainder it scales is below b
fn wide(a: u128, b: u128) -> Option<u128> {
    let (quotient, remainder) = (a / b, a % b);
    let whole = quotient.checked_mul(ONE_U)?;

    // b past this and even one 10^9 step overflows; rare enough to be worth a
    // slow exact path rather than a wider fast one
    if b > CHUNK_LIMIT {
        return long_division(whole, remainder, b);
    }

    let first = remainder * CHUNK;
    let second = (first % b) * CHUNK;
    let fraction = (first / b).checked_mul(CHUNK)?.checked_add(second / b)?;
    round_half_away(whole.checked_add(fraction)?, second % b, b)
}

// A divisor above 3.4e11 carrying more than 9 decimal places reaches here: no
// step of 10^9 fits any more, so the scaling walks the 60 bits of 10^18 one at
// a time. The window stays below the divisor, so doubling it or adding the
// remainder to it stays below twice the divisor, and so inside a u128 - the
// product remainder * 10^18 is never formed.
fn long_division(whole: u128, remainder: u128, b: u128) -> Option<u128> {
    let mut fraction: u128 = 0;
    let mut window: u128 = 0;

    for bit in (0..ONE_U.ilog2() + 1).rev() {
        fraction = fraction.checked_mul(2)?;
        window *= 2;
        if window >= b {
            window -= b;
            fraction = fraction.checked_add(1)?;
        }
        if (ONE_U >> bit) & 1 == 1 {
            window += remainder;
            if window >= b {
                window -= b;
                fraction = fraction.checked_add(1)?;
            }
        }
    }

    round_half_away(whole.checked_add(fraction)?, window, b)
}

// the crate rounds half away from zero everywhere: parse, round_dp and mul all
// do, so a quotient that lands exactly between two raws goes the same way
#[inline(always)]
fn round_half_away(total: u128, remainder: u128, divisor: u128) -> Option<u128> {
    // remainder * 2 could overflow, so the comparison moves the halving onto
    // the divisor and handles its odd case separately
    let half = divisor / 2;
    match remainder > half || (remainder == half && divisor.is_multiple_of(2)) {
        true => total.checked_add(1),
        false => Some(total),
    }
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
mod divide {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::Dec;
    use crate::dec;
    use std::str::FromStr;

    fn parse(text: &str) -> Dec {
        Dec::from_str(text).unwrap()
    }

    #[test]
    fn test_market_precision_takes_the_fast_path() {
        // every price, size and tick carries at most 9 decimals, so its raw is
        // a multiple of 10^9 and the divisor cancels before it is scaled
        for text in ["0.00135", "104237.25", "1", "0.000000001"] {
            let raw = parse(text).into_raw().unsigned_abs();
            assert!(raw.is_multiple_of(CHUNK), "{text}");
        }
        assert_eq!(
            dec!(104_237.25) / dec!(0.00135),
            parse("77212777.777777777777777778")
        );
    }

    #[test]
    fn test_finer_than_nine_decimals_takes_the_wide_path() {
        let fine = parse("0.0000000001");
        assert!(!fine.into_raw().unsigned_abs().is_multiple_of(CHUNK));
        assert_eq!(dec!(1) / fine, parse("10000000000"));
        assert_eq!(Dec::ONE / Dec::EPSILON, parse("1000000000000000000"));
    }

    #[test]
    fn test_a_huge_divisor_with_fine_decimals_takes_the_long_path() {
        // past this the 10^9 steps overflow and the scaling walks bit by bit
        let divisor = parse("400000000000.000000000000000001");
        assert!(divisor.into_raw().unsigned_abs() > CHUNK_LIMIT);
        assert_eq!(dec!(1) / divisor, parse("0.0000000000025"));
        assert_eq!(divisor / dec!(3), parse("133333333333.333333333333333334"));
    }

    #[test]
    fn test_the_bit_walk_agrees_with_the_chunked_steps() {
        // the two general paths compute the same function over disjoint
        // divisor ranges, so run the slow one inside the fast one's range
        for (left, right) in [
            ("1", "3"),
            ("2", "3"),
            ("104237.25", "0.0000000000007"),
            ("-7.5", "2.0000000000000001"),
            ("0.000000000000000001", "1.0000000000000003"),
            (
                "170141183460469231731.687303715884105727",
                "99999999.9999999999999",
            ),
        ] {
            let a = parse(left).into_raw().unsigned_abs();
            let b = parse(right).into_raw().unsigned_abs();
            assert!(b <= CHUNK_LIMIT, "{right} is outside the chunked range");
            let (quotient, remainder) = (a / b, a % b);
            let whole = quotient.checked_mul(ONE_U).unwrap();
            assert_eq!(
                wide(a, b),
                long_division(whole, remainder, b),
                "{left} / {right}"
            );
        }
    }

    #[test]
    fn test_the_paths_agree_over_random_inputs() {
        // the bit walk is independent of the 10^9 steps and of the fast path's
        // cancellation, so where their domains overlap each one checks the
        // others: a mistake would have to be made identically three times
        let mut state = 0x2545_F491_4F6C_DD1D_u64;
        let mut checked = 0;
        for _ in 0..20_000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let a = (state as u128) * (state as u128 | 1);
            let b = match state % 3 {
                // a divisor small enough for every path to handle it
                0 => (state >> 20) as u128 | 1,
                1 => ((state >> 20) as u128 | 1) * CHUNK,
                _ => (state >> 8) as u128 | 1,
            };
            if b > CHUNK_LIMIT || a > MAX_RAW as u128 {
                continue;
            }
            let whole = match (a / b).checked_mul(ONE_U) {
                Some(whole) => whole,
                None => continue,
            };
            let expected = long_division(whole, a % b, b);
            assert_eq!(wide(a, b), expected, "wide {a} / {b}");
            if b.is_multiple_of(CHUNK) {
                assert_eq!(scale_by(a, b / CHUNK, CHUNK), expected, "fast {a} / {b}");
            }
            checked += 1;
        }
        assert!(checked > 5_000, "only {checked} cases were in range");
    }

    #[test]
    fn test_ties_round_half_away_from_zero() {
        assert_eq!(dec!(1) / dec!(3), parse("0.333333333333333333"));
        assert_eq!(dec!(2) / dec!(3), parse("0.666666666666666667"));
        assert_eq!(dec!(1) / dec!(7), parse("0.142857142857142857"));
        // an exact half at the last place goes away from zero on both signs
        assert_eq!(Dec::from_raw(1) / dec!(2), Dec::from_raw(1));
        assert_eq!(Dec::from_raw(-1) / dec!(2), Dec::from_raw(-1));
        assert_eq!(Dec::from_raw(3) / dec!(2), Dec::from_raw(2));
    }

    #[test]
    fn test_signs_and_identities() {
        assert_eq!(dec!(10) / dec!(4), dec!(2.5));
        assert_eq!(dec!(-7.5) / dec!(2), dec!(-3.75));
        assert_eq!(dec!(7.5) / dec!(-2), dec!(-3.75));
        assert_eq!(dec!(-7.5) / dec!(-2), dec!(3.75));
        assert_eq!(dec!(7.5) / Dec::ONE, dec!(7.5));
        assert_eq!(dec!(7.5) / dec!(7.5), Dec::ONE);
        assert_eq!(Dec::ZERO / dec!(7.5), Dec::ZERO);
        assert_eq!(dec!(7.5) / Dec::NEG_ONE, dec!(-7.5));
        assert_eq!(
            Dec::MAX / dec!(2),
            parse("85070591730234615865.843651857942052864")
        );

        let mut value = dec!(12);
        value /= dec!(4);
        assert_eq!(value, dec!(3));
    }

    #[test]
    fn test_dividing_by_zero_is_nan_rather_than_infinity() {
        // the type has no infinity, so this is the same fault as an overflow
        assert!((dec!(1) / Dec::ZERO).is_nan());
        assert!((dec!(-1) / Dec::ZERO).is_nan());
        assert!((Dec::ZERO / Dec::ZERO).is_nan());
        assert_eq!(dec!(1).checked_div(Dec::ZERO), None);
        assert!(dec!(1).saturating_div(Dec::ZERO).is_nan());
    }

    #[test]
    fn test_nan_propagates_through_division() {
        assert!((Dec::NAN / dec!(2)).is_nan());
        assert!((dec!(2) / Dec::NAN).is_nan());
        assert_eq!(Dec::NAN.checked_div(dec!(2)), None);
        assert_eq!(dec!(2).checked_div(Dec::NAN), None);
        assert!(Dec::NAN.saturating_div(dec!(2)).is_nan());
    }

    #[test]
    fn test_a_quotient_past_the_range_overflows() {
        assert_eq!(Dec::MAX.checked_div(dec!(0.5)), None);
        assert_eq!(Dec::MAX.checked_div(Dec::EPSILON), None);
        assert!((Dec::MAX / dec!(0.5)).is_nan());
        assert_eq!(Dec::MAX.saturating_div(dec!(0.5)), Dec::MAX);
        assert_eq!(Dec::MAX.saturating_div(dec!(-0.5)), Dec::MIN);
        assert_eq!(Dec::MIN.saturating_div(dec!(0.5)), Dec::MIN);
    }

    #[test]
    fn test_dividing_then_multiplying_returns_the_value() {
        for (left, right) in [
            ("104237.25", "0.00135"),
            ("1", "8"),
            ("-12345.6789", "2.5"),
            ("0.000000000000000004", "2"),
        ] {
            let (a, b) = (parse(left), parse(right));
            assert_eq!((a / b) * b, a, "{left} / {right}");
        }
    }
}
