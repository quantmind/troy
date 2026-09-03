use super::core::Dec;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Mul, MulAssign, Neg, Sub, SubAssign};

/// The product, or [`Dec::NAN`] on overflow or from a NaN operand.
///
/// The finite range is +/-1.7e20 and no price, size or notional lives near it,
/// so an overflow here is a bug rather than a number: bad input, or an
/// accumulation that ran away. Saturating would answer it with a plausible
/// looking figure that survives every downstream check, so the operators
/// return NaN instead and carry the fault to wherever the result is finally
/// examined. [`Dec::checked_mul`] reports it, [`Dec::saturating_mul`] clamps
/// it, for callers who would rather decide on the spot.
impl Mul for Dec {
    type Output = Self;

    #[inline(always)]
    fn mul(self, rhs: Self) -> Self {
        match self.checked_mul(rhs) {
            Some(value) => value,
            None => Self::NAN,
        }
    }
}

impl MulAssign for Dec {
    #[inline(always)]
    fn mul_assign(&mut self, rhs: Self) {
        *self = *self * rhs;
    }
}

/// Negation, which is exact and total: the finite range is symmetric, so every
/// value has a negation, and the NaN pattern is its own.
impl Neg for Dec {
    type Output = Self;

    #[inline(always)]
    fn neg(self) -> Self {
        Self(self.0.wrapping_neg())
    }
}

/// The sum, or [`Dec::NAN`] on overflow or from a NaN operand. See
/// [`Dec::checked_add`] and [`Dec::saturating_add`] to handle it on the spot.
impl Add for Dec {
    type Output = Self;

    #[inline(always)]
    fn add(self, rhs: Self) -> Self {
        match self.checked_add(rhs) {
            Some(value) => value,
            None => Self::NAN,
        }
    }
}

/// The difference, or [`Dec::NAN`] on overflow or from a NaN operand. See
/// [`Dec::checked_sub`] and [`Dec::saturating_sub`] to handle it on the spot.
impl Sub for Dec {
    type Output = Self;

    #[inline(always)]
    fn sub(self, rhs: Self) -> Self {
        match self.checked_sub(rhs) {
            Some(value) => value,
            None => Self::NAN,
        }
    }
}

impl AddAssign for Dec {
    #[inline(always)]
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl SubAssign for Dec {
    #[inline(always)]
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl Sum for Dec {
    #[inline(always)]
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::ZERO, |total, value| total + value)
    }
}

impl<'a> Sum<&'a Dec> for Dec {
    #[inline(always)]
    fn sum<I: Iterator<Item = &'a Self>>(iter: I) -> Self {
        iter.fold(Self::ZERO, |total, value| total + *value)
    }
}

impl From<i64> for Dec {
    #[inline(always)]
    fn from(value: i64) -> Self {
        Self::from_int(value)
    }
}

impl From<i32> for Dec {
    #[inline(always)]
    fn from(value: i32) -> Self {
        Self::from_int(value as i64)
    }
}

impl From<u32> for Dec {
    #[inline(always)]
    fn from(value: u32) -> Self {
        Self::from_int(value as i64)
    }
}

impl From<u64> for Dec {
    #[inline(always)]
    fn from(value: u64) -> Self {
        Self::from_u64(value)
    }
}

#[cfg(test)]
mod conversions {
    #![allow(clippy::unwrap_used)]

    use crate::Dec;

    use crate::dec;

    #[test]
    fn test_integer_conversions() {
        assert_eq!(Dec::from_int(-7), dec!(-7));
        assert_eq!(Dec::from(3_i32), dec!(3));
        assert_eq!(Dec::from(3_u32), dec!(3));
        assert_eq!(Dec::from(3_i64), dec!(3));
        assert_eq!(Dec::from(3_u64), dec!(3));
        assert_eq!(Dec::from(i64::MAX).to_string(), "9223372036854775807");
        assert_eq!(Dec::from(u64::MAX).to_string(), "18446744073709551615");
    }

    #[test]
    fn test_raw_round_trip() {
        assert_eq!(Dec::from_raw(dec!(1.5).into_raw()), dec!(1.5));
        assert_eq!(Dec::ONE.into_raw(), 1_000_000_000_000_000_000);
    }

    #[test]
    fn test_the_reserved_pattern_is_nan() {
        assert_eq!(Dec::from_raw(i128::MIN), Dec::NAN);
        assert_eq!(Dec::from_raw(i128::MAX), Dec::MAX);
        assert!(Dec::NAN.is_nan());
        assert!(!Dec::NAN.is_finite());
        assert!(Dec::MIN.is_finite());
        assert!(Dec::MAX.is_finite());
        assert!(!Dec::NAN.is_sign_negative());
        assert!(!Dec::NAN.is_sign_positive());
        assert!(!Dec::NAN.is_zero());
    }

    #[test]
    fn test_the_finite_range_is_symmetric() {
        assert_eq!(-Dec::MIN, Dec::MAX);
        assert_eq!(-Dec::MAX, Dec::MIN);
        assert_eq!(Dec::MIN.into_raw(), -Dec::MAX.into_raw());
    }
}

#[cfg(test)]
mod arithmetic_methods {
    use crate::Dec;

    use crate::dec;

    #[test]
    fn test_default_is_zero() {
        assert_eq!(Dec::default(), Dec::ZERO);
    }

    #[test]
    fn test_checked_add_reports_overflow() {
        assert_eq!(dec!(1).checked_add(dec!(2)), Some(dec!(3)));
        assert_eq!(dec!(1.5).checked_add(dec!(-2.5)), Some(dec!(-1)));
        assert_eq!(Dec::MAX.checked_add(Dec::EPSILON), None);
        assert_eq!(Dec::MIN.checked_add(-Dec::EPSILON), None);
        assert_eq!(Dec::MAX.checked_add(Dec::ZERO), Some(Dec::MAX));
    }

    #[test]
    fn test_checked_sub_reports_overflow() {
        assert_eq!(dec!(3).checked_sub(dec!(1)), Some(dec!(2)));
        assert_eq!(Dec::MIN.checked_sub(Dec::EPSILON), None);
        assert_eq!(Dec::MAX.checked_sub(-Dec::EPSILON), None);
        assert_eq!(Dec::MIN.checked_sub(Dec::ZERO), Some(Dec::MIN));
    }

    #[test]
    fn test_saturating_methods_clamp_at_the_bounds() {
        assert_eq!(Dec::MAX.saturating_add(Dec::ONE), Dec::MAX);
        assert_eq!(Dec::MIN.saturating_sub(Dec::ONE), Dec::MIN);
        assert_eq!(dec!(1).saturating_add(dec!(2)), dec!(3));
        assert_eq!(dec!(1).saturating_sub(dec!(2)), dec!(-1));
    }

    #[test]
    fn test_assign_operators() {
        let mut value = dec!(1);
        value += dec!(0.5);
        assert_eq!(value, dec!(1.5));
        value -= dec!(2);
        assert_eq!(value, dec!(-0.5));

        let mut product = dec!(3);
        product *= dec!(4);
        assert_eq!(product, dec!(12));
    }

    // saturating would answer an overflow with a plausible looking figure
    #[test]
    fn test_overflow_becomes_nan() {
        assert!((Dec::MAX * dec!(2)).is_nan());
        assert!((Dec::MAX * Dec::MAX).is_nan());
        assert!((Dec::MAX + Dec::ONE).is_nan());
        assert!((Dec::MIN - Dec::ONE).is_nan());
    }

    #[test]
    fn test_nan_propagates_through_every_operation() {
        for value in [dec!(1), dec!(-1), Dec::ZERO, Dec::MAX, Dec::MIN] {
            assert!((Dec::NAN + value).is_nan(), "{value} + NaN");
            assert!((value + Dec::NAN).is_nan(), "NaN + {value}");
            assert!((Dec::NAN - value).is_nan(), "NaN - {value}");
            assert!((value - Dec::NAN).is_nan(), "{value} - NaN");
            assert!((Dec::NAN * value).is_nan(), "NaN * {value}");
            assert!((value * Dec::NAN).is_nan(), "{value} * NaN");
            assert!(Dec::NAN.midpoint(value).is_nan());
            assert!(Dec::NAN.saturating_add(value).is_nan());
            assert!(Dec::NAN.saturating_sub(value).is_nan());
            assert!(Dec::NAN.saturating_mul(value).is_nan());
            assert_eq!(Dec::NAN.checked_add(value), None);
            assert_eq!(Dec::NAN.checked_sub(value), None);
            assert_eq!(Dec::NAN.checked_mul(value), None);
        }
        assert!((-Dec::NAN).is_nan());
        assert!(Dec::NAN.abs().is_nan());
        assert!(Dec::NAN.signum().is_nan());
        assert!(Dec::NAN.floor().is_nan());
        assert!(Dec::NAN.ceil().is_nan());
        assert!(Dec::NAN.trunc().is_nan());
    }

    // NaN times zero is still NaN: the fault outranks the annihilator
    #[test]
    fn test_nan_times_zero_is_nan() {
        assert!((Dec::NAN * Dec::ZERO).is_nan());
        assert!((Dec::ZERO * Dec::NAN).is_nan());
    }

    #[test]
    fn test_nan_formats_orders_and_converts() {
        assert_eq!(Dec::NAN.to_string(), "NaN");
        assert_eq!(format!("{:?}", Dec::NAN), "NaN");
        assert!(Dec::NAN.to_f64().is_nan());
        // unlike an IEEE NaN this one is reflexive, which keeps Eq and Ord
        assert_eq!(Dec::NAN, Dec::NAN);
        assert!(Dec::NAN < Dec::MIN);
        assert_eq!(
            [Dec::ZERO, Dec::NAN, Dec::MIN].iter().min(),
            Some(&Dec::NAN)
        );
    }

    #[test]
    fn test_sum_over_values_and_references() {
        let values = [dec!(0.1), dec!(0.2), dec!(0.3)];
        assert_eq!(values.iter().copied().sum::<Dec>(), dec!(0.6));
        assert_eq!(values.iter().sum::<Dec>(), dec!(0.6));
        assert_eq!([].iter().sum::<Dec>(), Dec::ZERO);
    }
}
