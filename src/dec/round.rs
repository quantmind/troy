use super::core::{Dec, MAX_RAW, ONE_U, POW10, clamp_raw, div_round};

impl Dec {
        #[inline(always)]
    pub fn from_f64_round(value: f64, dp: u32) -> Option<Self> {
        Self::from_f64(value).map(|value| value.round_dp(dp))
    }

    #[inline(always)]
    pub const fn round_dp(self, dp: u32) -> Self {
        if dp >= Dec::SCALE {
            return self;
        }
        let factor = POW10[(Dec::SCALE - dp) as usize];
        Self(clamp_raw(div_round(self.0, factor).saturating_mul(factor)))
    }

    #[inline(always)]
    pub const fn round_to_step(self, step: Self) -> Self {
        if step.0 <= 0 {
            return self;
        }
        match pow10_exponent(step.0) {
            Some(exponent) if exponent <= Dec::SCALE => {
                Self(round_to_places(self.0, Dec::SCALE - exponent))
            }
            _ => Self(clamp_raw(div_round(self.0, step.0).saturating_mul(step.0))),
        }
    }
}

// floor(log10) from the bit length, 1233/4096 approximating log10(2). The
// estimate is exact or one low, so a single correction settles it
const fn pow10_exponent(value: i128) -> Option<u32> {
    let bits = 128 - value.leading_zeros();
    let exponent = (bits * 1233) >> 12;
    if POW10[exponent as usize] == value {
        return Some(exponent);
    }
    if exponent + 1 < POW10.len() as u32 && POW10[(exponent + 1) as usize] == value {
        return Some(exponent + 1);
    }
    None
}

// a step of 10^-dp divides one whole unit, so only the fraction moves, and it
// moves inside a u64 where a constant divisor is a multiply. A fraction that
// rounds up to a whole unit carries into the integer part
#[inline(always)]
const fn round_to_places(raw: i128, dp: u32) -> i128 {
    if dp >= Dec::SCALE {
        return raw;
    }
    let magnitude = raw.unsigned_abs();
    let fraction = round_fraction((magnitude % ONE_U) as u64, dp) as u128;
    let scaled = match (magnitude / ONE_U).checked_mul(ONE_U) {
        Some(integer) => match integer.checked_add(fraction) {
            Some(value) => value,
            None => u128::MAX,
        },
        None => u128::MAX,
    };
    // the range is symmetric, so one limit serves both signs
    let limit = MAX_RAW as u128;
    let clamped = if scaled > limit { limit } else { scaled };
    match raw < 0 {
        true => -(clamped as i128),
        false => clamped as i128,
    }
}

#[inline(always)]
const fn round_fraction(fraction: u64, dp: u32) -> u64 {
    match dp {
        0 => round_u64::<1_000_000_000_000_000_000>(fraction),
        1 => round_u64::<100_000_000_000_000_000>(fraction),
        2 => round_u64::<10_000_000_000_000_000>(fraction),
        3 => round_u64::<1_000_000_000_000_000>(fraction),
        4 => round_u64::<100_000_000_000_000>(fraction),
        5 => round_u64::<10_000_000_000_000>(fraction),
        6 => round_u64::<1_000_000_000_000>(fraction),
        7 => round_u64::<100_000_000_000>(fraction),
        8 => round_u64::<10_000_000_000>(fraction),
        9 => round_u64::<1_000_000_000>(fraction),
        10 => round_u64::<100_000_000>(fraction),
        11 => round_u64::<10_000_000>(fraction),
        12 => round_u64::<1_000_000>(fraction),
        13 => round_u64::<100_000>(fraction),
        14 => round_u64::<10_000>(fraction),
        15 => round_u64::<1_000>(fraction),
        16 => round_u64::<100>(fraction),
        17 => round_u64::<10>(fraction),
        _ => fraction,
    }
}

// halves round away from zero, matching round_dp
#[inline(always)]
const fn round_u64<const STEP: u64>(fraction: u64) -> u64 {
    let quotient = fraction / STEP;
    let remainder = fraction % STEP;
    match remainder * 2 >= STEP {
        true => (quotient + 1) * STEP,
        false => quotient * STEP,
    }
}

#[cfg(test)]
mod round_to_step {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::Dec;

    use crate::dec;

    const VALUES: [Dec; 9] = [
        Dec::ZERO,
        dec!(0.5),
        dec!(-0.5),
        dec!(2.5),
        dec!(1.234567),
        dec!(-1.234567),
        dec!(1234.98765),
        Dec::MAX,
        Dec::MIN,
    ];

    #[test]
    fn test_powers_of_ten_match_round_dp() {
        for value in VALUES {
            for dp in 0..=Dec::SCALE {
                let step = Dec::from_raw(POW10[(Dec::SCALE - dp) as usize]).unwrap();
                assert_eq!(
                    value.round_to_step(step),
                    value.round_dp(dp),
                    "value {value} step {step}"
                );
            }
        }
    }

    #[test]
    fn test_arbitrary_steps() {
        assert_eq!(dec!(1.6543).round_to_step(dec!(0.25)), dec!(1.75));
        assert_eq!(dec!(-1.6543).round_to_step(dec!(0.25)), dec!(-1.75));
        assert_eq!(dec!(103).round_to_step(dec!(5)), dec!(105));
        assert_eq!(dec!(102).round_to_step(dec!(5)), dec!(100));
    }

    #[test]
    fn test_halves_round_away_from_zero() {
        assert_eq!(dec!(1.125).round_to_step(dec!(0.01)), dec!(1.13));
        assert_eq!(dec!(-1.125).round_to_step(dec!(0.01)), dec!(-1.13));
        assert_eq!(dec!(0.5).round_to_step(Dec::ONE), Dec::ONE);
        assert_eq!(dec!(-0.5).round_to_step(Dec::ONE), Dec::NEG_ONE);
    }

    #[test]
    fn test_a_rounded_up_fraction_carries() {
        assert_eq!(dec!(1.999).round_to_step(dec!(0.01)), dec!(2));
        assert_eq!(dec!(-1.999).round_to_step(dec!(0.01)), dec!(-2));
        assert_eq!(dec!(0.9999).round_to_step(dec!(0.001)), Dec::ONE);
    }

    #[test]
    fn test_a_step_of_ten_or_more_rounds_the_integer_part() {
        assert_eq!(dec!(104).round_to_step(dec!(10)), dec!(100));
        assert_eq!(dec!(105).round_to_step(dec!(10)), dec!(110));
        assert_eq!(dec!(1449).round_to_step(dec!(100)), dec!(1400));
    }

    #[test]
    fn test_a_non_positive_step_is_a_no_op() {
        assert_eq!(dec!(1.5).round_to_step(Dec::ZERO), dec!(1.5));
        assert_eq!(dec!(1.5).round_to_step(dec!(-0.01)), dec!(1.5));
    }

    #[test]
    fn test_the_extremes_saturate() {
        assert_eq!(Dec::MAX.round_to_step(Dec::ONE), Dec::MAX);
        assert_eq!(Dec::MIN.round_to_step(Dec::ONE), Dec::MIN);
        assert_eq!(Dec::MAX.round_to_step(dec!(0.25)), Dec::MAX);
        assert_eq!(Dec::MIN.round_to_step(dec!(0.25)), Dec::MIN);
    }

    #[test]
    fn test_pow10_exponent_finds_every_power() {
        for (exponent, power) in POW10.iter().enumerate() {
            assert_eq!(pow10_exponent(*power), Some(exponent as u32));
        }
        assert_eq!(pow10_exponent(25), None);
        assert_eq!(pow10_exponent(999), None);
        assert_eq!(pow10_exponent(i128::MAX), None);
    }
}
