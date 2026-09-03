use super::core::Dec;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Mul, MulAssign, Neg, Sub, SubAssign};

// The range is +/-1.7e20 and no price, size or notional lives near it, so an
// overflow here is a bug rather than a number: bad input, or an accumulation
// that ran away. Saturating would answer it with a plausible looking figure
// that survives every downstream check, so the operators panic instead.
// `checked_*` reports it, `saturating_*` clamps it, for callers who want to
// decide for themselves.
#[cold]
#[inline(never)]
#[allow(clippy::panic)]
fn overflowed(operation: &str, lhs: Dec, rhs: Dec) -> ! {
    panic!("Dec {operation} overflowed: {lhs}, {rhs}")
}

impl Mul for Dec {
    type Output = Self;

    #[inline(always)]
    fn mul(self, rhs: Self) -> Self {
        match self.checked_mul(rhs) {
            Some(value) => value,
            None => overflowed("multiplication", self, rhs),
        }
    }
}

impl MulAssign for Dec {
    #[inline(always)]
    fn mul_assign(&mut self, rhs: Self) {
        *self = *self * rhs;
    }
}

// exact and total: the range is symmetric, so every value has a negation
impl Neg for Dec {
    type Output = Self;

    #[inline(always)]
    fn neg(self) -> Self {
        Self(-self.0)
    }
}

impl Add for Dec {
    type Output = Self;

    #[inline(always)]
    fn add(self, rhs: Self) -> Self {
        debug_assert!(
            self.checked_add(rhs).is_some(),
            "Dec addition overflowed: {self} + {rhs}"
        );
        self.saturating_add(rhs)
    }
}

impl Sub for Dec {
    type Output = Self;

    #[inline(always)]
    fn sub(self, rhs: Self) -> Self {
        debug_assert!(
            self.checked_sub(rhs).is_some(),
            "Dec subtraction overflowed: {self} - {rhs}"
        );
        self.saturating_sub(rhs)
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
        assert_eq!(Dec::from_raw(dec!(1.5).into_raw()), Some(dec!(1.5)));
        assert_eq!(Dec::ONE.into_raw(), 1_000_000_000_000_000_000);
    }

    #[test]
    fn test_i128_min_is_the_one_raw_outside_the_range() {
        assert_eq!(Dec::from_raw(i128::MIN), None);
        assert_eq!(Dec::from_raw(i128::MAX), Some(Dec::MAX));
        assert_eq!(Dec::from_raw(Dec::MIN.into_raw()), Some(Dec::MIN));

        assert_eq!(Dec::from_raw_saturating(i128::MIN), Dec::MIN);
        assert_eq!(Dec::from_raw_saturating(i128::MAX), Dec::MAX);
    }

    #[test]
    fn test_the_range_is_symmetric() {
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
    #[should_panic(expected = "Dec multiplication overflowed")]
    fn test_the_multiplication_operator_panics_on_overflow() {
        let _ = Dec::MAX * dec!(2);
    }

    // addition and subtraction check only in debug, keeping the hot path branchless
    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "Dec addition overflowed")]
    fn test_the_addition_operator_panics_on_overflow_in_debug() {
        let _ = Dec::MAX + Dec::ONE;
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "Dec subtraction overflowed")]
    fn test_the_subtraction_operator_panics_on_overflow_in_debug() {
        let _ = Dec::MIN - Dec::ONE;
    }

    #[test]
    fn test_sum_over_values_and_references() {
        let values = [dec!(0.1), dec!(0.2), dec!(0.3)];
        assert_eq!(values.iter().copied().sum::<Dec>(), dec!(0.6));
        assert_eq!(values.iter().sum::<Dec>(), dec!(0.6));
        assert_eq!([].iter().sum::<Dec>(), Dec::ZERO);
    }
}
