use super::core::{Dec, MAX_RAW, NAN_RAW, ONE_U, POW10, dec_or_nan};

/// How a value that does not land on a step is resolved.
///
/// The `Midpoint` variants round to the nearest step and differ only in how
/// they settle an exact tie. The rest are directed: they resolve every inexact
/// value the same way, however close it sits to either side.
///
/// [`Dec::round_dp`] and [`Dec::round_to_step`] round
/// [`MidpointAwayFromZero`](RoundingStrategy::MidpointAwayFromZero), which is
/// also the [`Default`], so a policy stored per instrument starts where the
/// plain methods do.
///
/// ```
/// use troy::{RoundingStrategy, dec};
///
/// let price = dec!(104_237.286);
/// assert_eq!(price.round_dp_with(2, RoundingStrategy::ToZero), dec!(104_237.28));
/// assert_eq!(price.round_dp_with(2, RoundingStrategy::ToPositiveInfinity), dec!(104_237.29));
/// ```
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum RoundingStrategy {
    /// To the nearest step, a tie away from zero. The default, and what a
    /// price coming out of a model wants when it is pinned to a tick.
    #[default]
    MidpointAwayFromZero,
    /// To the nearest step, a tie to the even step. Banker's rounding: use it
    /// where many rounded values are summed, such as settlement or an
    /// accumulating PnL, and the away-from-zero bias would compound.
    MidpointNearestEven,
    /// Towards zero, discarding the rest. What an exchange quantity filter
    /// does to a size, and so the safe direction for an order that must not
    /// exceed a balance.
    ToZero,
    /// Away from zero, always up in magnitude. For a margin, a fee or a buffer
    /// that has to cover the exact value.
    AwayFromZero,
    /// Down, towards negative infinity. Floor: the bid-side price rounding, so
    /// a quote never creeps up into the spread.
    ToNegativeInfinity,
    /// Up, towards positive infinity. Ceil: the ask-side counterpart.
    ToPositiveInfinity,
}

impl Dec {
    /// [`Dec::from_f64`] followed by [`Dec::round_dp`], which is how a float
    /// carrying binary representation error is best pinned to a known scale.
    #[inline(always)]
    pub fn from_f64_round(value: f64, dp: u32) -> Option<Self> {
        Self::from_f64(value).map(|value| value.round_dp(dp))
    }

    /// Round to `dp` decimal places, halves away from zero. A no-op once `dp`
    /// reaches [`Dec::SCALE`]. Returns [`Dec::NAN`] when the rounded value
    /// leaves the finite range, as it does for [`Dec::MIN`] at `dp` 0, and
    /// when the input is already NaN.
    ///
    /// ```
    /// use troy::{Dec, dec};
    ///
    /// assert_eq!(dec!(2.5).round_dp(0), dec!(3));
    /// assert_eq!(dec!(-2.5).round_dp(0), dec!(-3));
    /// assert!(Dec::MAX.round_dp(0).is_nan());
    /// ```
    #[inline(always)]
    pub const fn round_dp(self, dp: u32) -> Self {
        self.round_dp_with(dp, RoundingStrategy::MidpointAwayFromZero)
    }

    /// [`Dec::round_dp`] with the fraction resolved by `strategy` rather than
    /// half away from zero. Every other rule is the same: a no-op once `dp`
    /// reaches [`Dec::SCALE`], and [`Dec::NAN`] for a NaN input or a result
    /// that leaves the finite range.
    ///
    /// ```
    /// use troy::{RoundingStrategy, dec};
    ///
    /// let size = dec!(1.2999);
    /// assert_eq!(size.round_dp_with(2, RoundingStrategy::ToZero), dec!(1.29));
    /// assert_eq!(size.round_dp_with(2, RoundingStrategy::AwayFromZero), dec!(1.30));
    /// assert_eq!(dec!(2.5).round_dp_with(0, RoundingStrategy::MidpointNearestEven), dec!(2));
    /// assert_eq!(dec!(3.5).round_dp_with(0, RoundingStrategy::MidpointNearestEven), dec!(4));
    /// ```
    #[inline(always)]
    pub const fn round_dp_with(self, dp: u32, strategy: RoundingStrategy) -> Self {
        if dp >= Dec::SCALE {
            // a no-op at the native scale, NaN included
            return self;
        }
        if self.is_nan() {
            return Self::NAN;
        }
        let factor = POW10[(Dec::SCALE - dp) as usize];
        dec_or_nan(div_strategy(self.0, factor, strategy).checked_mul(factor))
    }

    /// Round to the nearest multiple of `step`, halves away from zero. A
    /// non-positive step is a no-op. Returns [`Dec::NAN`] when the result
    /// leaves the finite range, and when either side is already NaN.
    ///
    /// ```
    /// use troy::dec;
    ///
    /// assert_eq!(dec!(104_237.28).round_to_step(dec!(0.25)), dec!(104_237.25));
    /// ```
    #[inline(always)]
    pub const fn round_to_step(self, step: Self) -> Self {
        self.round_to_step_with(step, RoundingStrategy::MidpointAwayFromZero)
    }

    /// [`Dec::round_to_step`] with the fraction resolved by `strategy` rather
    /// than half away from zero. A non-positive step is still a no-op, and the
    /// NaN rules are unchanged.
    ///
    /// ```
    /// use troy::{RoundingStrategy, dec};
    ///
    /// // a tick a bid may take without crossing, and the ask-side counterpart
    /// let tick = dec!(0.25);
    /// let fair = dec!(104_237.28);
    /// assert_eq!(fair.round_to_step_with(tick, RoundingStrategy::ToNegativeInfinity), dec!(104_237.25));
    /// assert_eq!(fair.round_to_step_with(tick, RoundingStrategy::ToPositiveInfinity), dec!(104_237.50));
    /// ```
    #[inline(always)]
    pub const fn round_to_step_with(self, step: Self, strategy: RoundingStrategy) -> Self {
        if self.is_nan() || step.is_nan() {
            return Self::NAN;
        }
        if step.0 <= 0 {
            return self;
        }
        match pow10_exponent(step.0) {
            Some(exponent) if exponent <= Dec::SCALE => {
                Self(round_to_places(self.0, Dec::SCALE - exponent, strategy))
            }
            _ => dec_or_nan(div_strategy(self.0, step.0, strategy).checked_mul(step.0)),
        }
    }
}

// `numerator / divisor` with the fraction resolved by `strategy`, for a
// positive divisor. Halving the divisor rather than doubling the remainder
// keeps the comparison inside an i128 for a divisor at the top of the range.
// An odd divisor has no exact half, which is why evenness is part of the tie
// rather than the remainder comparison alone.
//
// `round_u64` decides the same question for the fast path, written out again
// in that width rather than shared. A helper covering both has to take every
// strategy's terms as arguments, and computing the ones the chosen strategy
// then discards costs a measurable instruction on each of these hot paths.
// What holds the two together is the property that sends every strategy down
// both and compares them, not shared code.
#[inline(always)]
pub(crate) const fn div_strategy(
    numerator: i128,
    divisor: i128,
    strategy: RoundingStrategy,
) -> i128 {
    let quotient = numerator / divisor;
    let remainder = numerator % divisor;
    if remainder == 0 {
        return quotient;
    }
    let magnitude = if remainder < 0 { -remainder } else { remainder };
    let half = divisor / 2;
    let away = match strategy {
        RoundingStrategy::MidpointAwayFromZero => {
            magnitude > half || (magnitude == half && divisor & 1 == 0)
        }
        RoundingStrategy::MidpointNearestEven => {
            magnitude > half || (magnitude == half && divisor & 1 == 0 && quotient & 1 != 0)
        }
        RoundingStrategy::ToZero => false,
        RoundingStrategy::AwayFromZero => true,
        RoundingStrategy::ToNegativeInfinity => numerator < 0,
        RoundingStrategy::ToPositiveInfinity => numerator > 0,
    };
    if !away {
        return quotient;
    }
    if numerator < 0 {
        quotient.saturating_sub(1)
    } else {
        quotient.saturating_add(1)
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
// rounds up to a whole unit carries into the integer part. The sign travels
// separately because the directed strategies need it: rounding towards
// negative infinity grows the magnitude of a negative value and shrinks a
// positive one.
#[inline(always)]
const fn round_to_places(raw: i128, dp: u32, strategy: RoundingStrategy) -> i128 {
    if dp >= Dec::SCALE {
        return raw;
    }
    let negative = raw < 0;
    let magnitude = raw.unsigned_abs();
    let fraction = round_fraction((magnitude % ONE_U) as u64, dp, negative, strategy) as u128;
    let scaled = match (magnitude / ONE_U).checked_mul(ONE_U) {
        Some(integer) => match integer.checked_add(fraction) {
            Some(value) => value,
            None => u128::MAX,
        },
        None => u128::MAX,
    };
    // the finite range is symmetric, so one limit serves both signs
    if scaled > MAX_RAW as u128 {
        return NAN_RAW;
    }
    match negative {
        true => -(scaled as i128),
        false => scaled as i128,
    }
}

#[inline(always)]
const fn round_fraction(fraction: u64, dp: u32, negative: bool, strategy: RoundingStrategy) -> u64 {
    match dp {
        0 => round_u64::<1_000_000_000_000_000_000>(fraction, negative, strategy),
        1 => round_u64::<100_000_000_000_000_000>(fraction, negative, strategy),
        2 => round_u64::<10_000_000_000_000_000>(fraction, negative, strategy),
        3 => round_u64::<1_000_000_000_000_000>(fraction, negative, strategy),
        4 => round_u64::<100_000_000_000_000>(fraction, negative, strategy),
        5 => round_u64::<10_000_000_000_000>(fraction, negative, strategy),
        6 => round_u64::<1_000_000_000_000>(fraction, negative, strategy),
        7 => round_u64::<100_000_000_000>(fraction, negative, strategy),
        8 => round_u64::<10_000_000_000>(fraction, negative, strategy),
        9 => round_u64::<1_000_000_000>(fraction, negative, strategy),
        10 => round_u64::<100_000_000>(fraction, negative, strategy),
        11 => round_u64::<10_000_000>(fraction, negative, strategy),
        12 => round_u64::<1_000_000>(fraction, negative, strategy),
        13 => round_u64::<100_000>(fraction, negative, strategy),
        14 => round_u64::<10_000>(fraction, negative, strategy),
        15 => round_u64::<1_000>(fraction, negative, strategy),
        16 => round_u64::<100>(fraction, negative, strategy),
        17 => round_u64::<10>(fraction, negative, strategy),
        _ => fraction,
    }
}

// the magnitude of the fraction against a constant step, the decision left to
// `steps_away` so this path and the i128 one settle a tie the same way
#[inline(always)]
const fn round_u64<const STEP: u64>(
    fraction: u64,
    negative: bool,
    strategy: RoundingStrategy,
) -> u64 {
    let quotient = fraction / STEP;
    let remainder = fraction % STEP;
    let half = STEP / 2;
    // An exact fraction never moves, whatever the strategy, and a directed one
    // would carry it to the next step without the first term. That term is a
    // conjunct rather than an early return because every step here is an even
    // constant no smaller than ten: under the nearest strategies the whole
    // expression folds back to the single comparison against half a step this
    // path has always been, with no branch on the data added.
    let away = remainder != 0
        && match strategy {
            RoundingStrategy::MidpointAwayFromZero => remainder >= half,
            RoundingStrategy::MidpointNearestEven => {
                remainder > half || (remainder == half && quotient & 1 != 0)
            }
            RoundingStrategy::ToZero => false,
            RoundingStrategy::AwayFromZero => true,
            RoundingStrategy::ToNegativeInfinity => negative,
            RoundingStrategy::ToPositiveInfinity => !negative,
        };
    match away {
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
                let step = Dec::from_raw(POW10[(Dec::SCALE - dp) as usize]);
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
    fn test_the_extremes_become_nan() {
        assert!(Dec::MAX.round_to_step(Dec::ONE).is_nan());
        assert!(Dec::MIN.round_to_step(Dec::ONE).is_nan());
        assert!(Dec::MAX.round_to_step(dec!(0.25)).is_nan());
        assert!(Dec::MIN.round_to_step(dec!(0.25)).is_nan());
    }

    #[test]
    fn test_nan_survives_both_rounding_paths() {
        assert!(Dec::NAN.round_to_step(Dec::ONE).is_nan());
        assert!(Dec::NAN.round_to_step(dec!(0.25)).is_nan());
        assert!(dec!(1.5).round_to_step(Dec::NAN).is_nan());
        assert!(Dec::NAN.round_dp(2).is_nan());
        assert!(Dec::NAN.round_dp(Dec::SCALE).is_nan());
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

#[cfg(test)]
mod strategies {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::Dec;
    use crate::dec;

    use RoundingStrategy::*;

    const ALL: [RoundingStrategy; 6] = [
        MidpointAwayFromZero,
        MidpointNearestEven,
        ToZero,
        AwayFromZero,
        ToNegativeInfinity,
        ToPositiveInfinity,
    ];

    #[test]
    fn test_each_strategy_resolves_a_fraction_its_own_way() {
        // .284 is below the half, .286 above it, so only the directed
        // strategies tell the two apart
        for (value, expected) in [
            (
                dec!(1.284),
                [
                    dec!(1.28),
                    dec!(1.28),
                    dec!(1.28),
                    dec!(1.29),
                    dec!(1.28),
                    dec!(1.29),
                ],
            ),
            (
                dec!(1.286),
                [
                    dec!(1.29),
                    dec!(1.29),
                    dec!(1.28),
                    dec!(1.29),
                    dec!(1.28),
                    dec!(1.29),
                ],
            ),
            (
                dec!(-1.284),
                [
                    dec!(-1.28),
                    dec!(-1.28),
                    dec!(-1.28),
                    dec!(-1.29),
                    dec!(-1.29),
                    dec!(-1.28),
                ],
            ),
            (
                dec!(-1.286),
                [
                    dec!(-1.29),
                    dec!(-1.29),
                    dec!(-1.28),
                    dec!(-1.29),
                    dec!(-1.29),
                    dec!(-1.28),
                ],
            ),
        ] {
            for (strategy, expected) in ALL.into_iter().zip(expected) {
                assert_eq!(
                    value.round_dp_with(2, strategy),
                    expected,
                    "{value} at 2 places {strategy:?}"
                );
            }
        }
    }

    #[test]
    fn test_a_tie_splits_the_two_nearest_strategies() {
        // the tie is the only case they disagree on, and nearest-even settles
        // it by the parity of the step below, not of the value
        assert_eq!(dec!(0.5).round_dp_with(0, MidpointNearestEven), Dec::ZERO);
        assert_eq!(dec!(1.5).round_dp_with(0, MidpointNearestEven), dec!(2));
        assert_eq!(dec!(2.5).round_dp_with(0, MidpointNearestEven), dec!(2));
        assert_eq!(dec!(3.5).round_dp_with(0, MidpointNearestEven), dec!(4));
        assert_eq!(dec!(-2.5).round_dp_with(0, MidpointNearestEven), dec!(-2));
        assert_eq!(dec!(-3.5).round_dp_with(0, MidpointNearestEven), dec!(-4));

        assert_eq!(dec!(2.5).round_dp_with(0, MidpointAwayFromZero), dec!(3));
        assert_eq!(dec!(-2.5).round_dp_with(0, MidpointAwayFromZero), dec!(-3));
    }

    #[test]
    fn test_nearest_even_holds_on_an_odd_step() {
        // an odd step has no exact half, so the tie never arises and the two
        // nearest strategies agree everywhere
        for raw in -40_i128..=40 {
            let value = Dec::from_raw(raw);
            let step = Dec::from_raw(3);
            assert_eq!(
                value.round_to_step_with(step, MidpointNearestEven),
                value.round_to_step_with(step, MidpointAwayFromZero),
                "raw {raw} to a step of 3"
            );
        }
    }

    #[test]
    fn test_the_zero_place_strategies_are_floor_ceil_and_trunc() {
        for value in [
            dec!(2.5),
            dec!(-2.5),
            dec!(1.0001),
            dec!(-1.0001),
            dec!(7),
            Dec::ZERO,
        ] {
            assert_eq!(
                value.round_dp_with(0, ToNegativeInfinity),
                value.floor(),
                "{value}"
            );
            assert_eq!(
                value.round_dp_with(0, ToPositiveInfinity),
                value.ceil(),
                "{value}"
            );
            assert_eq!(value.round_dp_with(0, ToZero), value.trunc(), "{value}");
        }
    }

    #[test]
    fn test_the_default_is_what_the_plain_methods_do() {
        assert_eq!(RoundingStrategy::default(), MidpointAwayFromZero);
        for value in [
            dec!(1.125),
            dec!(-1.125),
            dec!(104_237.286),
            Dec::MAX,
            Dec::MIN,
        ] {
            for dp in 0..=Dec::SCALE {
                assert_eq!(
                    value.round_dp_with(dp, RoundingStrategy::default()),
                    value.round_dp(dp),
                    "{value} at {dp} places"
                );
            }
            assert_eq!(
                value.round_to_step_with(dec!(0.25), RoundingStrategy::default()),
                value.round_to_step(dec!(0.25)),
                "{value} to a step of 0.25"
            );
        }
    }

    #[test]
    fn test_an_exact_value_never_moves() {
        for strategy in ALL {
            for value in [dec!(1.25), dec!(-1.25), dec!(100), Dec::ZERO] {
                assert_eq!(
                    value.round_to_step_with(dec!(0.25), strategy),
                    value,
                    "{value} {strategy:?}"
                );
            }
        }
    }

    #[test]
    fn test_a_directed_strategy_carries_into_the_integer_part() {
        assert_eq!(dec!(1.991).round_dp_with(2, ToPositiveInfinity), dec!(2));
        assert_eq!(dec!(-1.991).round_dp_with(2, ToNegativeInfinity), dec!(-2));
        assert_eq!(dec!(0.9991).round_dp_with(3, AwayFromZero), Dec::ONE);
        assert_eq!(dec!(-0.9991).round_dp_with(3, AwayFromZero), Dec::NEG_ONE);
    }

    #[test]
    fn test_towards_zero_never_leaves_the_range() {
        // the magnitude only shrinks, so the one strategy that cannot overflow
        // answers where the others give up
        for value in [Dec::MAX, Dec::MIN] {
            assert!(value.round_dp_with(0, ToZero).is_finite(), "{value}");
            assert!(value.round_dp_with(0, AwayFromZero).is_nan(), "{value}");
        }
        assert!(Dec::MAX.round_dp_with(0, ToPositiveInfinity).is_nan());
        assert!(Dec::MAX.round_dp_with(0, ToNegativeInfinity).is_finite());
        assert!(Dec::MIN.round_dp_with(0, ToNegativeInfinity).is_nan());
        assert!(Dec::MIN.round_dp_with(0, ToPositiveInfinity).is_finite());
    }

    #[test]
    fn test_nan_survives_every_strategy() {
        for strategy in ALL {
            assert!(Dec::NAN.round_dp_with(2, strategy).is_nan(), "{strategy:?}");
            assert!(
                Dec::NAN.round_dp_with(Dec::SCALE, strategy).is_nan(),
                "{strategy:?}"
            );
            assert!(
                Dec::NAN.round_to_step_with(Dec::ONE, strategy).is_nan(),
                "{strategy:?}"
            );
            assert!(
                Dec::NAN.round_to_step_with(dec!(0.25), strategy).is_nan(),
                "{strategy:?}"
            );
            assert!(
                dec!(1.5).round_to_step_with(Dec::NAN, strategy).is_nan(),
                "{strategy:?}"
            );
        }
    }

    #[test]
    fn test_a_non_positive_step_stays_a_no_op() {
        for strategy in ALL {
            assert_eq!(dec!(1.5).round_to_step_with(Dec::ZERO, strategy), dec!(1.5));
            assert_eq!(
                dec!(1.5).round_to_step_with(dec!(-0.01), strategy),
                dec!(1.5)
            );
        }
    }
}
