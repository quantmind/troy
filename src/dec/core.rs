use super::mul::{div_exact_1e9, mul_raw};
use super::parse::parse_bytes;

pub(crate) const ONE_RAW: i128 = 1_000_000_000_000_000_000;

pub(crate) const ONE_U: u128 = 1_000_000_000_000_000_000;

pub(crate) const MAX_RAW: i128 = i128::MAX;

// i128::MIN has no positive counterpart, so admitting it would make negation
// and abs partial and force a 2^127 special case into every magnitude check.
// Reserving it costs one value in 2^128 and buys a symmetric range.
pub(crate) const MIN_RAW: i128 = -i128::MAX;

// i128::MIN is the only raw outside the range, so one compare settles it
#[inline(always)]
pub(crate) const fn clamp_raw(raw: i128) -> i128 {
    match raw < MIN_RAW {
        true => MIN_RAW,
        false => raw,
    }
}

#[inline(always)]
pub(crate) const fn check_raw(raw: i128) -> Option<i128> {
    match raw < MIN_RAW {
        true => None,
        false => Some(raw),
    }
}

const CHUNK_F64: f64 = 1e9;

pub(crate) const POW10: [i128; 39] = {
    let mut table = [1_i128; 39];
    let mut index = 1;
    while index < 39 {
        table[index] = table[index - 1] * 10;
        index += 1;
    }
    table
};

#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Dec(pub(crate) i128);

impl Dec {
    /// Number of decimal places every `Dec` carries.
    pub const SCALE: u32 = 18;

    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(ONE_RAW);
    pub const NEG_ONE: Self = Self(-ONE_RAW);
    pub const MIN: Self = Self(MIN_RAW);
    pub const MAX: Self = Self(MAX_RAW);
    pub const EPSILON: Self = Self(1);

    /// Wrap a raw scaled integer, or `None` for `i128::MIN`, the one value
    /// outside the range. Every raw that [`Dec::into_raw`] returns round trips.
    #[inline(always)]
    pub const fn from_raw(raw: i128) -> Option<Self> {
        match check_raw(raw) {
            Some(raw) => Some(Self(raw)),
            None => None,
        }
    }

    /// Wrap a raw scaled integer, clamping `i128::MIN` to [`Dec::MIN`].
    #[inline(always)]
    pub const fn from_raw_saturating(raw: i128) -> Self {
        Self(clamp_raw(raw))
    }

    #[inline(always)]
    pub const fn into_raw(self) -> i128 {
        self.0
    }

    #[inline(always)]
    pub const fn from_int(value: i64) -> Self {
        Self(value as i128 * ONE_RAW)
    }

    #[inline(always)]
    pub const fn from_u64(value: u64) -> Self {
        Self(value as i128 * ONE_RAW)
    }

    #[inline(always)]
    pub const fn parse_const(value: &str) -> Option<Self> {
        match parse_bytes(value.as_bytes()) {
            Ok(raw) => Some(Self(raw)),
            Err(_) => None,
        }
    }

    #[inline(always)]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    #[inline(always)]
    pub const fn is_sign_negative(self) -> bool {
        self.0 < 0
    }

    #[inline(always)]
    pub const fn is_sign_positive(self) -> bool {
        self.0 > 0
    }

    #[inline(always)]
    pub const fn abs(self) -> Self {
        Self(self.0.saturating_abs())
    }

    #[inline(always)]
    pub const fn signum(self) -> Self {
        Self(self.0.signum() * ONE_RAW)
    }

    #[inline(always)]
    pub fn to_f64(self) -> f64 {
        // i128 -> f64 is a __floattidf libcall, i64 -> f64 is one instruction.
        // A value with at most 9 decimals divides exactly by 10^9, and the
        // quotient fits an i64 for anything under 9.2e9, which is every price
        // and size; the remaining scale then divides in f64.
        if let Some(scaled) = div_exact_1e9(self.0.unsigned_abs())
            && let Ok(narrow) = i64::try_from(scaled)
        {
            let signed = match self.0 < 0 {
                true => -narrow,
                false => narrow,
            };
            return signed as f64 / CHUNK_F64;
        }
        self.0 as f64 / ONE_RAW as f64
    }

    #[inline(always)]
    pub fn from_f64(value: f64) -> Option<Self> {
        // NaN would otherwise cast to zero; every other out-of-range value,
        // the infinities included, is caught by the checked arithmetic below
        if value.is_nan() {
            return None;
        }
        let integer = value.trunc();
        // exact: the subtraction cannot round, and the product stays under 10^18
        let fraction = ((value - integer) * ONE_RAW as f64).round() as i64;
        (integer as i128)
            .checked_mul(ONE_RAW)?
            .checked_add(trim_f64_noise(fraction) as i128)
            .and_then(check_raw)
            .map(Self)
    }

    #[inline(always)]
    pub const fn floor(self) -> Self {
        Self(clamp_raw(
            self.0.div_euclid(ONE_RAW).saturating_mul(ONE_RAW),
        ))
    }

    #[inline(always)]
    pub const fn ceil(self) -> Self {
        // the leading negation is exact: the range is symmetric
        Self(clamp_raw(
            self.0
                .saturating_neg()
                .div_euclid(ONE_RAW)
                .saturating_mul(ONE_RAW)
                .saturating_neg(),
        ))
    }

    #[inline(always)]
    pub const fn trunc(self) -> Self {
        Self(clamp_raw((self.0 / ONE_RAW).saturating_mul(ONE_RAW)))
    }

    #[inline(always)]
    pub const fn checked_add(self, rhs: Self) -> Option<Self> {
        match self.0.checked_add(rhs.0) {
            Some(raw) => Self::from_raw(raw),
            None => None,
        }
    }

    #[inline(always)]
    pub const fn checked_sub(self, rhs: Self) -> Option<Self> {
        match self.0.checked_sub(rhs.0) {
            Some(raw) => Self::from_raw(raw),
            None => None,
        }
    }

    #[inline(always)]
    pub fn checked_mul(self, rhs: Self) -> Option<Self> {
        mul_raw(self.0, rhs.0).map(Self)
    }

    #[inline(always)]
    pub fn saturating_mul(self, rhs: Self) -> Self {
        match self.checked_mul(rhs) {
            Some(value) => value,
            None => match (self.0 < 0) != (rhs.0 < 0) {
                true => Self::MIN,
                false => Self::MAX,
            },
        }
    }

    #[inline(always)]
    pub const fn saturating_add(self, rhs: Self) -> Self {
        Self(clamp_raw(self.0.saturating_add(rhs.0)))
    }

    #[inline(always)]
    pub const fn saturating_sub(self, rhs: Self) -> Self {
        Self(clamp_raw(self.0.saturating_sub(rhs.0)))
    }

    // cannot overflow: the sum is formed in a wider space than either operand.
    // A half unit in the last place rounds towards zero rather than away from
    // it, the one place this type does not round halves away from zero.
    #[inline(always)]
    pub const fn midpoint(self, rhs: Self) -> Self {
        Self(self.0.midpoint(rhs.0))
    }
}

// an f64 carries 15 significant digits; a scaled fraction holding more has
// invented the rest, so drop them. Only four cases arise, each with a constant
// divisor the compiler turns into a multiply.
const fn trim_f64_noise(fraction: i64) -> i64 {
    let magnitude = fraction.unsigned_abs();
    if magnitude >= 100_000_000_000_000_000 {
        div_round_i64(fraction, 1_000).saturating_mul(1_000)
    } else if magnitude >= 10_000_000_000_000_000 {
        div_round_i64(fraction, 100).saturating_mul(100)
    } else if magnitude >= 1_000_000_000_000_000 {
        div_round_i64(fraction, 10).saturating_mul(10)
    } else {
        fraction
    }
}

const fn div_round_i64(numerator: i64, divisor: i64) -> i64 {
    let quotient = numerator / divisor;
    let remainder = numerator % divisor;
    let magnitude = if remainder < 0 { -remainder } else { remainder };
    if magnitude * 2 < divisor {
        return quotient;
    }
    if numerator < 0 {
        quotient.saturating_sub(1)
    } else {
        quotient.saturating_add(1)
    }
}

pub(crate) const fn div_round(numerator: i128, divisor: i128) -> i128 {
    let quotient = numerator / divisor;
    let remainder = numerator % divisor;
    if remainder == 0 {
        return quotient;
    }
    let magnitude = if remainder < 0 { -remainder } else { remainder };
    let half = divisor / 2;
    let round_away = magnitude > half || (magnitude == half && divisor & 1 == 0);
    if !round_away {
        return quotient;
    }
    if numerator < 0 {
        quotient.saturating_sub(1)
    } else {
        quotient.saturating_add(1)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use crate::Dec;
    use crate::ParseDecError;
    use crate::dec;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::str::FromStr;

    #[test]
    fn test_parse_and_display_round_trip() {
        for text in [
            "0",
            "1",
            "-1",
            "0.5",
            "-0.5",
            "100000.5",
            "0.000000000000000001",
            "-12345.6789",
        ] {
            let value = Dec::from_str(text).unwrap();
            assert_eq!(value.to_string(), text, "round trip of {text}");
        }
    }

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
            Dec::from_raw(2).unwrap()
        );
    }

    #[test]
    fn test_parse_exponent_form() {
        assert_eq!(
            Dec::from_str("1e-8").unwrap(),
            Dec::from_raw(10_000_000_000).unwrap()
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
        assert_eq!(HALF, Dec::from_raw(500_000_000_000_000_000).unwrap());
        assert_eq!(NEGATIVE.to_string(), "-1.25");
    }

    #[test]
    fn test_representation_is_canonical() {
        let one = Dec::from_str("1.0").unwrap();
        let one_padded = Dec::from_str("1.000000").unwrap();
        assert_eq!(one, one_padded);

        let hash = |value: Dec| {
            let mut hasher = DefaultHasher::new();
            value.hash(&mut hasher);
            hasher.finish()
        };
        assert_eq!(hash(one), hash(one_padded));
    }

    #[test]
    fn test_addition_is_exact() {
        let tenth = dec!(0.1);
        let sum: Dec = std::iter::repeat_n(tenth, 10).sum();
        assert_eq!(sum, Dec::ONE);
    }

    #[test]
    fn test_f64_round_trip() {
        for text in ["0.5", "104237.25", "-0.00135", "0"] {
            let value = Dec::from_str(text).unwrap();
            assert_eq!(Dec::from_f64(value.to_f64()).unwrap(), value);
        }
        assert_eq!(Dec::from_f64(f64::NAN), None);
        assert_eq!(Dec::from_f64(f64::INFINITY), None);
        assert_eq!(Dec::from_f64(f64::NEG_INFINITY), None);
        assert_eq!(Dec::from_f64(1e30), None);
        assert_eq!(Dec::from_f64(-1e30), None);
        assert_eq!(Dec::from_f64(1e20).unwrap().to_f64(), 1e20);
        assert_eq!(Dec::from_f64(1e-18), Some(Dec::EPSILON));
    }

    #[test]
    fn test_round_dp_rounds_halves_away_from_zero() {
        assert_eq!(dec!(2.5).round_dp(0), dec!(3));
        assert_eq!(dec!(3.5).round_dp(0), dec!(4));
        assert_eq!(dec!(-2.5).round_dp(0), dec!(-3));
        assert_eq!(dec!(1.234567).round_dp(4), dec!(1.2346));
        assert_eq!(
            dec!(1.5).round_dp(18),
            dec!(1.5),
            "no-op at the native scale"
        );
    }

    #[test]
    fn test_floor_ceil_trunc() {
        assert_eq!(dec!(1.5).floor(), dec!(1));
        assert_eq!(dec!(-1.5).floor(), dec!(-2));
        assert_eq!(dec!(1.5).ceil(), dec!(2));
        assert_eq!(dec!(-1.5).ceil(), dec!(-1));
        assert_eq!(dec!(-1.5).trunc(), dec!(-1));
    }

    #[test]
    fn test_overflow_is_reported_or_clamped_on_request() {
        assert_eq!(Dec::MAX.checked_add(Dec::ONE), None);
        assert_eq!(Dec::MIN.checked_sub(Dec::ONE), None);
        assert_eq!(Dec::MAX.saturating_add(Dec::ONE), Dec::MAX);
        assert_eq!(Dec::MIN.saturating_sub(Dec::ONE), Dec::MIN);
    }

    #[test]
    fn test_extremes_do_not_overflow() {
        assert_eq!(Dec::MIN.abs(), Dec::MAX);
        assert_eq!(-Dec::MIN, Dec::MAX);
        assert_eq!(Dec::MIN.floor(), Dec::MIN);
        assert_eq!(Dec::MAX.ceil(), Dec::MAX);
        assert_eq!(Dec::MIN.round_dp(0), Dec::MIN);
    }
}

#[cfg(test)]
mod to_f64_range {
    #![allow(clippy::unwrap_used)]

    use crate::Dec;

    use std::str::FromStr;

    #[test]
    fn test_to_f64_is_total() {
        for value in [Dec::MIN, Dec::MAX, Dec::EPSILON, -Dec::EPSILON, Dec::ZERO] {
            let converted = value.to_f64();
            assert!(converted.is_finite(), "{value} -> {converted}");
        }
        assert_eq!(Dec::ZERO.to_f64(), 0.0);
        assert_eq!(Dec::EPSILON.to_f64(), 1e-18);
    }

    #[test]
    fn test_to_f64_loses_digits_beyond_f64_precision() {
        // 39 significant digits going into a type that holds ~16
        let wide = Dec::MAX;
        assert_ne!(Dec::from_f64(wide.to_f64()), Some(wide));
        // values inside f64's precision survive the trip
        let narrow = Dec::from_str("104237.25").unwrap();
        assert_eq!(Dec::from_f64(narrow.to_f64()), Some(narrow));
    }
}

#[cfg(test)]
mod midpoint {
    #![allow(clippy::unwrap_used)]

    use crate::Dec;

    use crate::dec;

    #[test]
    fn test_midpoint_of_two_values() {
        assert_eq!(dec!(100).midpoint(dec!(102)), dec!(101));
        assert_eq!(
            dec!(104_237.20).midpoint(dec!(104_237.30)),
            dec!(104_237.25)
        );
        assert_eq!(dec!(1).midpoint(dec!(2)), dec!(1.5));
        assert_eq!(dec!(5).midpoint(dec!(5)), dec!(5));
    }

    #[test]
    fn test_midpoint_is_commutative_and_handles_signs() {
        assert_eq!(dec!(-4).midpoint(dec!(2)), dec!(-1));
        assert_eq!(dec!(2).midpoint(dec!(-4)), dec!(-1));
        assert_eq!(dec!(-3).midpoint(dec!(-5)), dec!(-4));
    }

    #[test]
    fn test_midpoint_cannot_overflow() {
        assert_eq!(Dec::MAX.midpoint(Dec::MAX), Dec::MAX);
        assert_eq!(Dec::MIN.midpoint(Dec::MIN), Dec::MIN);
        assert_eq!(Dec::MIN.midpoint(Dec::MAX), Dec::ZERO);
    }

    #[test]
    fn test_midpoint_of_an_odd_last_place_rounds_towards_zero() {
        assert_eq!(
            Dec::from_raw(1)
                .unwrap()
                .midpoint(Dec::from_raw(2).unwrap()),
            Dec::from_raw(1).unwrap()
        );
        assert_eq!(
            Dec::from_raw(-1)
                .unwrap()
                .midpoint(Dec::from_raw(-2).unwrap()),
            Dec::from_raw(-1).unwrap()
        );
    }
}

#[cfg(test)]
mod from_f64_round {
    #![allow(clippy::unwrap_used)]

    use crate::Dec;

    use crate::dec;

    #[test]
    fn test_rounds_to_the_requested_places() {
        assert_eq!(Dec::from_f64_round(104_237.256, 2), Some(dec!(104_237.26)));
        assert_eq!(Dec::from_f64_round(104_237.254, 2), Some(dec!(104_237.25)));
        assert_eq!(Dec::from_f64_round(1.0 / 3.0, 4), Some(dec!(0.3333)));
        assert_eq!(Dec::from_f64_round(2.5, 0), Some(dec!(3)));
        assert_eq!(Dec::from_f64_round(-2.5, 0), Some(dec!(-3)));
    }

    #[test]
    fn test_absorbs_binary_representation_error() {
        // 0.1 + 0.2 is 0.30000000000000004 in binary
        assert_eq!(Dec::from_f64_round(0.1 + 0.2, 2), Some(dec!(0.3)));
        assert_eq!(Dec::from_f64_round(0.1 + 0.2, 18), Some(dec!(0.3)));
    }

    #[test]
    fn test_beyond_the_native_scale_matches_from_f64() {
        for value in [0.5_f64, 104_237.25, -0.00135, 0.0] {
            assert_eq!(
                Dec::from_f64_round(value, Dec::SCALE),
                Dec::from_f64(value),
                "{value}"
            );
        }
    }

    #[test]
    fn test_rejects_what_from_f64_rejects() {
        assert_eq!(Dec::from_f64_round(f64::NAN, 2), None);
        assert_eq!(Dec::from_f64_round(f64::INFINITY, 2), None);
        assert_eq!(Dec::from_f64_round(1e30, 2), None);
    }
}

#[cfg(test)]
mod predicates {

    use crate::Dec;

    use crate::dec;

    #[test]
    fn test_sign_predicates() {
        assert!(Dec::ZERO.is_zero());
        assert!(!dec!(0.000000000000000001).is_zero());

        assert!(dec!(-1).is_sign_negative());
        assert!(!dec!(1).is_sign_negative());
        assert!(!Dec::ZERO.is_sign_negative());

        assert!(dec!(1).is_sign_positive());
        assert!(!dec!(-1).is_sign_positive());
        // zero is neither, unlike f64 where the sign bit decides
        assert!(!Dec::ZERO.is_sign_positive());
    }

    #[test]
    fn test_signum_and_abs() {
        assert_eq!(dec!(42.5).signum(), Dec::ONE);
        assert_eq!(dec!(-42.5).signum(), Dec::NEG_ONE);
        assert_eq!(Dec::ZERO.signum(), Dec::ZERO);

        assert_eq!(dec!(-42.5).abs(), dec!(42.5));
        assert_eq!(dec!(42.5).abs(), dec!(42.5));
        // the magnitude of MIN is not representable, so it saturates
        assert_eq!(Dec::MIN.abs(), Dec::MAX);
    }

    #[test]
    fn test_debug_matches_display() {
        for value in [dec!(1.5), dec!(-0.25), Dec::ZERO, Dec::MIN, Dec::MAX] {
            assert_eq!(format!("{value:?}"), format!("{value}"));
        }
    }
}

#[cfg(test)]
mod ordering {
    #![allow(clippy::unwrap_used)]

    use crate::Dec;

    use crate::dec;
    use std::collections::BTreeMap;
    use std::str::FromStr;

    #[test]
    fn test_sorts_by_numeric_value() {
        let mut values = vec![
            dec!(2),
            dec!(-1.5),
            dec!(0),
            dec!(10),
            dec!(1.05),
            dec!(1.5),
        ];
        values.sort();
        assert_eq!(
            values,
            vec![
                dec!(-1.5),
                dec!(0),
                dec!(1.05),
                dec!(1.5),
                dec!(2),
                dec!(10)
            ]
        );
    }

    #[test]
    fn test_ordering_is_independent_of_written_precision() {
        assert_eq!(dec!(1.5).cmp(&dec!(1.50)), std::cmp::Ordering::Equal);
        assert!(dec!(1.05) < dec!(1.1));
        assert!(dec!(-2) < dec!(-1));
        assert!(Dec::MIN < Dec::MAX);
        assert_eq!(dec!(1).max(dec!(2)), dec!(2));
        assert_eq!(dec!(1).clamp(dec!(2), dec!(3)), dec!(2));
    }

    #[test]
    fn test_serves_as_an_ordered_map_key() {
        let mut book = BTreeMap::new();
        for (price, size) in [("104237.30", 3), ("104237.25", 1), ("104237.28", 2)] {
            book.insert(Dec::from_str(price).unwrap(), size);
        }
        let sizes: Vec<_> = book.values().copied().collect();
        assert_eq!(sizes, vec![1, 2, 3], "iteration follows price order");

        let best = book.keys().next().unwrap();
        assert_eq!(*best, Dec::from_str("104237.25").unwrap());

        let range: Vec<_> = book
            .range(Dec::from_str("104237.26").unwrap()..)
            .map(|(_, size)| *size)
            .collect();
        assert_eq!(range, vec![2, 3]);
    }
}

#[cfg(test)]
mod to_f64_accuracy {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use std::str::FromStr;

    fn ulps_from_truth(value: Dec) -> i64 {
        // f64's own parser is correctly rounded, so the string round trip is
        // the true nearest f64 to this decimal
        let truth: f64 = value.to_string().parse().unwrap();
        (value.to_f64().to_bits() as i64 - truth.to_bits() as i64).abs()
    }

    #[test]
    fn test_stays_within_one_ulp_of_the_true_nearest_f64() {
        let mut state = 0x2545_F491_4F6C_DD1D_u64;
        let mut worst = 0;
        for _ in 0..50_000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            // 0 to 9 decimal places, the shape the narrow path targets
            let scale = POW10[(state % 10) as usize];
            let sign = if state & 1 == 0 { 1 } else { -1 };
            let raw = ((state as i128) * scale).saturating_mul(sign);
            worst = worst.max(ulps_from_truth(Dec::from_raw(raw).unwrap()));
        }
        assert!(worst <= 1, "drifted {worst} ulp from the nearest f64");
    }

    #[test]
    fn test_the_narrow_and_wide_paths_agree() {
        // more than 9 decimals cannot divide by 10^9, so it takes the wide path
        for text in [
            "0.0000000001",
            "1.234567890123456789",
            "0.000000000000000001",
        ] {
            let value = Dec::from_str(text).unwrap();
            assert!(
                div_exact_1e9(value.into_raw().unsigned_abs()).is_none(),
                "{text}"
            );
            assert!(ulps_from_truth(value) <= 1, "{text}");
        }
        for text in ["104237.25", "0.00135", "-9223372036.854775807"] {
            let value = Dec::from_str(text).unwrap();
            assert!(ulps_from_truth(value) <= 1, "{text}");
        }
    }
}
