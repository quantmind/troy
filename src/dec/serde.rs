use super::core::{Dec, ONE_RAW};
use super::parse::ParseDecError;
use std::fmt;
use std::str::FromStr;

impl serde::Serialize for Dec {
    #[inline(always)]
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for Dec {
    #[inline(always)]
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(DecVisitor)
    }
}

struct DecVisitor;

impl serde::de::Visitor<'_> for DecVisitor {
    type Value = Dec;

    #[inline(always)]
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a decimal string or number")
    }

    #[inline(always)]
    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Dec, E> {
        Dec::from_str(value).map_err(serde::de::Error::custom)
    }

    #[inline(always)]
    fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Dec, E> {
        Dec::from_f64(value).ok_or_else(|| serde::de::Error::custom(ParseDecError::Overflow))
    }

    #[inline(always)]
    fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Dec, E> {
        Ok(Dec::from_int(value))
    }

    #[inline(always)]
    fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Dec, E> {
        Ok(Dec::from_u64(value))
    }

    #[inline(always)]
    fn visit_i128<E: serde::de::Error>(self, value: i128) -> Result<Dec, E> {
        value
            .checked_mul(ONE_RAW)
            .map(Dec)
            .ok_or_else(|| serde::de::Error::custom(ParseDecError::Overflow))
    }

    #[inline(always)]
    fn visit_u128<E: serde::de::Error>(self, value: u128) -> Result<Dec, E> {
        i128::try_from(value)
            .ok()
            .and_then(|value| value.checked_mul(ONE_RAW))
            .map(Dec)
            .ok_or_else(|| serde::de::Error::custom(ParseDecError::Overflow))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use crate::Dec;

    use crate::dec;
    use std::str::FromStr;

    #[test]
    fn test_serde_uses_a_decimal_string() {
        let value = dec!(104_237.25);
        let encoded = serde_json::to_string(&value).unwrap();
        assert_eq!(encoded, "\"104237.25\"");
        assert_eq!(serde_json::from_str::<Dec>(&encoded).unwrap(), value);
    }

    #[test]
    fn test_serde_accepts_bare_json_numbers() {
        assert_eq!(serde_json::from_str::<Dec>("0.5").unwrap(), dec!(0.5));
        assert_eq!(serde_json::from_str::<Dec>("7").unwrap(), dec!(7));
    }

    #[test]
    fn test_wide_values_round_trip_through_serde() {
        let encoded = serde_json::to_string(&Dec::MAX).unwrap();
        assert_eq!(serde_json::from_str::<Dec>(&encoded).unwrap(), Dec::MAX);
    }

    #[test]
    fn test_the_widest_values_survive_serde() {
        for value in [Dec::MIN, Dec::MAX] {
            let encoded = serde_json::to_string(&value).unwrap();
            assert_eq!(serde_json::from_str::<Dec>(&encoded).unwrap(), value);
        }
    }

    #[test]
    fn test_json_numbers_above_i64_max_are_accepted() {
        // representable, and accepted as a string, so the number form must agree
        for text in ["10000000000000000000", "18446744073709551615"] {
            assert_eq!(
                serde_json::from_str::<Dec>(text).unwrap(),
                Dec::from_str(text).unwrap(),
                "{text}"
            );
        }
    }
}
