use super::core::{POW10, div_round};
use super::{Dec, ParseDecError};
use rust_decimal::Decimal;

impl Dec {
    /// Rescale a [`Decimal`], exact unless it carries more than [`Dec::SCALE`]
    /// decimal places, where the excess rounds half away from zero.
    pub fn from_decimal(value: Decimal) -> Option<Self> {
        let scale = value.scale();
        let mantissa = value.mantissa();
        if scale <= Dec::SCALE {
            return mantissa
                .checked_mul(POW10[(Dec::SCALE - scale) as usize])
                .map(Self::from_raw);
        }
        let factor = match POW10.get((scale - Dec::SCALE) as usize) {
            Some(factor) => *factor,
            None => return Some(Self::ZERO),
        };
        Some(Self::from_raw(div_round(mantissa, factor)))
    }

    /// Widen to a [`Decimal`] at [`Dec::SCALE`] decimal places, or `None` when the
    /// value needs more than the 96 bits a `Decimal` mantissa holds.
    pub fn to_decimal(self) -> Option<Decimal> {
        Decimal::try_from_i128_with_scale(self.into_raw(), Dec::SCALE).ok()
    }
}

impl TryFrom<Decimal> for Dec {
    type Error = ParseDecError;

    fn try_from(value: Decimal) -> Result<Self, Self::Error> {
        Dec::from_decimal(value).ok_or(ParseDecError::Overflow)
    }
}

impl TryFrom<Dec> for Decimal {
    type Error = ParseDecError;

    fn try_from(value: Dec) -> Result<Self, Self::Error> {
        value.to_decimal().ok_or(ParseDecError::Overflow)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::dec;
    use std::str::FromStr;

    fn decimal(text: &str) -> Decimal {
        Decimal::from_str(text).unwrap()
    }

    #[test]
    fn test_round_trip_of_values_inside_both_ranges() {
        for text in [
            "0",
            "1",
            "-1",
            "0.5",
            "-0.5",
            "123.456",
            "-12345.6789",
            "0.000000000000000001",
            "79228162514.264337593543950335",
        ] {
            let value = Dec::from_decimal(decimal(text)).unwrap();
            assert_eq!(value, Dec::from_str(text).unwrap(), "from {text}");
            assert_eq!(value.to_decimal().unwrap(), decimal(text), "to {text}");
        }
    }

    #[test]
    fn test_excess_places_round_half_away_from_zero() {
        assert_eq!(
            Dec::from_decimal(decimal("1.0000000000000000005")).unwrap(),
            dec!(1.000000000000000001)
        );
        assert_eq!(
            Dec::from_decimal(decimal("-1.0000000000000000005")).unwrap(),
            dec!(-1.000000000000000001)
        );
        assert_eq!(
            Dec::from_decimal(decimal("1.0000000000000000004")).unwrap(),
            dec!(1)
        );
    }

    #[test]
    fn test_a_decimal_beyond_the_dec_range_overflows() {
        assert_eq!(Dec::from_decimal(Decimal::MAX), None);
        assert_eq!(Dec::from_decimal(Decimal::MIN), None);
        assert_eq!(
            Dec::try_from(Decimal::MAX).unwrap_err(),
            ParseDecError::Overflow
        );
    }

    #[test]
    fn test_a_dec_beyond_the_decimal_mantissa_overflows() {
        assert_eq!(Dec::MAX.to_decimal(), None);
        assert_eq!(Dec::MIN.to_decimal(), None);
        assert_eq!(
            Decimal::try_from(Dec::MAX).unwrap_err(),
            ParseDecError::Overflow
        );
    }

    #[test]
    fn test_conversions_are_available_as_traits() {
        let value: Dec = Decimal::from_str("2.5").unwrap().try_into().unwrap();
        assert_eq!(value, dec!(2.5));
        let back: Decimal = value.try_into().unwrap();
        assert_eq!(back, decimal("2.5"));
    }
}
