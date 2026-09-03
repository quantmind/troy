use super::core::Dec;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Mul, MulAssign, Neg, Sub, SubAssign};

impl Mul for Dec {
    type Output = Self;

    #[inline(always)]
    fn mul(self, rhs: Self) -> Self {
        self.saturating_mul(rhs)
    }
}

impl MulAssign for Dec {
    #[inline(always)]
    fn mul_assign(&mut self, rhs: Self) {
        *self = *self * rhs;
    }
}

impl Neg for Dec {
    type Output = Self;

    #[inline(always)]
    fn neg(self) -> Self {
        Self(self.0.saturating_neg())
    }
}

impl Add for Dec {
    type Output = Self;

    #[inline(always)]
    fn add(self, rhs: Self) -> Self {
        self.saturating_add(rhs)
    }
}

impl Sub for Dec {
    type Output = Self;

    #[inline(always)]
    fn sub(self, rhs: Self) -> Self {
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
        assert_eq!(Dec::from_raw(dec!(1.5).into_raw()), dec!(1.5));
        assert_eq!(Dec::ONE.into_raw(), 1_000_000_000_000_000_000);
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

        let mut saturating = Dec::MAX;
        saturating += Dec::ONE;
        assert_eq!(saturating, Dec::MAX);
    }

    #[test]
    fn test_sum_over_values_and_references() {
        let values = [dec!(0.1), dec!(0.2), dec!(0.3)];
        assert_eq!(values.iter().copied().sum::<Dec>(), dec!(0.6));
        assert_eq!(values.iter().sum::<Dec>(), dec!(0.6));
        assert_eq!([].iter().sum::<Dec>(), Dec::ZERO);
    }
}
