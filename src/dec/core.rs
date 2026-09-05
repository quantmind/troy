use super::div::div_raw;
use super::mul::{div_exact_1e9, mul_raw};
use super::parse::parse_bytes;

pub(crate) const ONE_RAW: i128 = 1_000_000_000_000_000_000;

pub(crate) const ONE_U: u128 = 1_000_000_000_000_000_000;

pub(crate) const MAX_RAW: i128 = i128::MAX;

// i128::MIN has no positive counterpart, so admitting it as a finite value
// would make negation and abs partial. Held out of the finite range it becomes
// the one spare bit pattern, and costs one value in 2^128 to reserve.
pub(crate) const MIN_RAW: i128 = -i128::MAX;

// The not-a-number state: overflow, and any operation touching one. It is the
// only raw below MIN_RAW, so a single compare recognises it, and because the
// finite range is symmetric it is also the only raw that wrapping negation and
// wrapping abs map to themselves - propagation through `-` and `abs` is free.
pub(crate) const NAN_RAW: i128 = i128::MIN;

// the finite range is symmetric, so NAN_RAW is the only raw outside it
#[inline(always)]
pub(crate) const fn clamp_raw(raw: i128) -> i128 {
    match raw < MIN_RAW {
        true => MIN_RAW,
        false => raw,
    }
}

// the result of a multiply that may overflow, or may land on the one raw the
// finite range excludes; either way it is not a number
#[inline(always)]
pub(crate) const fn dec_or_nan(raw: Option<i128>) -> Dec {
    match raw {
        Some(raw) if raw != NAN_RAW => Dec(raw),
        _ => Dec::NAN,
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

/// A decimal carrying a fixed [`Dec::SCALE`] decimal places, backed by an
/// `i128` holding the value scaled by `10^SCALE`.
///
/// Arithmetic is exact between [`Dec::MIN`] and [`Dec::MAX`]. Anything that
/// leaves that range becomes [`Dec::NAN`] and stays NaN through every later
/// operation, so the fault reaches the boundary where [`Dec::is_finite`] is
/// checked rather than being clamped to a plausible number or raised as a
/// panic on a hot path.
///
/// The ordering is total: NaN equals itself and sorts below [`Dec::MIN`],
/// which keeps `Eq`, `Ord` and `Hash` derivable and so keeps `Dec` usable as a
/// `BTreeMap` or `HashMap` key. See the [design](crate::design) notes for why.
///
/// ```
/// use troy::{Dec, dec};
///
/// assert_eq!(dec!(2.5) + dec!(0.25), dec!(2.75));
/// assert!((Dec::MAX + Dec::ONE).is_nan());
/// ```
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Dec(pub(crate) i128);

impl Dec {
    /// Number of decimal places every `Dec` carries.
    pub const SCALE: u32 = 18;

    /// Zero.
    pub const ZERO: Self = Self(0);
    /// One.
    pub const ONE: Self = Self(ONE_RAW);
    /// Negative one.
    pub const NEG_ONE: Self = Self(-ONE_RAW);

    /// The smallest finite value, `-Dec::MAX`. The finite range is symmetric,
    /// so negation and [`Dec::abs`] are total and exact on it.
    pub const MIN: Self = Self(MIN_RAW);

    /// The largest finite value, roughly `1.7e20`.
    pub const MAX: Self = Self(MAX_RAW);

    /// The smallest positive value, one unit in the last place.
    pub const EPSILON: Self = Self(1);

    /// The not-a-number state. Every operation that leaves the finite range
    /// returns it, and every operation given it returns it, so an invalid
    /// result carries its own invalidity to wherever it is finally checked
    /// with [`Dec::is_finite`].
    ///
    /// Unlike an IEEE NaN this one is ordered and reflexive: it equals itself
    /// and sorts below [`Dec::MIN`], which is what keeps `Eq` and `Ord`
    /// available. It therefore wins any `min` reduction and sorts to the front
    /// of a collection.
    ///
    /// ```
    /// use troy::Dec;
    ///
    /// assert_eq!(Dec::NAN, Dec::NAN);
    /// assert!(Dec::NAN < Dec::MIN);
    /// assert!(!Dec::NAN.is_finite());
    /// ```
    pub const NAN: Self = Self(NAN_RAW);

    /// Wrap a raw scaled integer. Total: `i128::MIN` is [`Dec::NAN`], so every
    /// raw round trips through [`Dec::into_raw`].
    #[inline(always)]
    pub const fn from_raw(raw: i128) -> Self {
        Self(raw)
    }

    /// Whether this is [`Dec::NAN`], the state every overflow collapses to.
    #[inline(always)]
    pub const fn is_nan(self) -> bool {
        self.0 == NAN_RAW
    }

    /// Whether this is an ordinary number, the check to make where a value
    /// leaves the system.
    ///
    /// ```
    /// use troy::{Dec, dec};
    ///
    /// assert!(dec!(1.5).is_finite());
    /// assert!(Dec::MAX.is_finite());
    /// assert!(!(Dec::MAX * Dec::MAX).is_finite());
    /// ```
    #[inline(always)]
    pub const fn is_finite(self) -> bool {
        self.0 != NAN_RAW
    }

    #[inline(always)]
    /// The underlying scaled integer, the inverse of [`Dec::from_raw`].
    pub const fn into_raw(self) -> i128 {
        self.0
    }

    #[inline(always)]
    /// An exact whole number. Every `i64` fits the finite range.
    pub const fn from_int(value: i64) -> Self {
        Self(value as i128 * ONE_RAW)
    }

    #[inline(always)]
    /// An exact whole number. Every `u64` fits the finite range.
    pub const fn from_u64(value: u64) -> Self {
        Self(value as i128 * ONE_RAW)
    }

    #[inline(always)]
    /// Parse in a `const` context, `None` on malformed or out-of-range text.
    /// The [`dec!`](crate::dec) macro wraps this.
    pub const fn parse_const(value: &str) -> Option<Self> {
        match parse_bytes(value.as_bytes()) {
            Ok(raw) => Some(Self(raw)),
            Err(_) => None,
        }
    }

    /// Whether this is exactly zero. [`Dec::NAN`] is not.
    #[inline(always)]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// Whether this is a finite value below zero. [`Dec::NAN`] is neither
    /// negative nor positive, so this is not a finiteness test.
    #[inline(always)]
    pub const fn is_sign_negative(self) -> bool {
        self.0 < 0 && self.is_finite()
    }

    /// Whether this is a value above zero. [`Dec::NAN`] is not.
    #[inline(always)]
    pub const fn is_sign_positive(self) -> bool {
        self.0 > 0
    }

    /// The magnitude, exact for every finite value because the range is
    /// symmetric. [`Dec::NAN`] stays NaN.
    #[inline(always)]
    pub const fn abs(self) -> Self {
        // wrapping_abs maps NAN_RAW to itself; every finite raw has an exact
        // magnitude, so this is exact and propagates without a branch
        Self(self.0.wrapping_abs())
    }

    /// [`Dec::ONE`], [`Dec::NEG_ONE`] or [`Dec::ZERO`] by sign. [`Dec::NAN`]
    /// stays NaN.
    #[inline(always)]
    pub const fn signum(self) -> Self {
        match self.is_nan() {
            true => Self::NAN,
            false => Self(self.0.signum() * ONE_RAW),
        }
    }

    /// Convert to `f64`, rounding to the nearest representable double.
    /// [`Dec::NAN`] becomes `f64::NAN`.
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
        // 10^9 does not divide 2^127, so NaN never takes the path above and
        // the test costs nothing on it
        if self.is_nan() {
            return f64::NAN;
        }
        self.0 as f64 / ONE_RAW as f64
    }

    /// Convert from `f64`, or `None` when the value is not finite or does not
    /// fit the finite range. This never yields [`Dec::NAN`]: a conversion
    /// reports failure directly, since there is no earlier computation for a
    /// NaN to have propagated from.
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

    /// The largest whole number at or below this value, or [`Dec::NAN`] when
    /// that leaves the finite range, as it does for [`Dec::MIN`].
    ///
    /// The `dp` 0 case of
    /// [`RoundingStrategy::ToNegativeInfinity`](crate::RoundingStrategy).
    #[inline(always)]
    pub const fn floor(self) -> Self {
        // NaN needs no test of its own: flooring 2^127 rounds away from zero,
        // and the product then overflows, which is already the NaN answer
        dec_or_nan(self.0.div_euclid(ONE_RAW).checked_mul(ONE_RAW))
    }

    /// The smallest whole number at or above this value, or [`Dec::NAN`] when
    /// that leaves the finite range, as it does for [`Dec::MAX`].
    ///
    /// The `dp` 0 case of
    /// [`RoundingStrategy::ToPositiveInfinity`](crate::RoundingStrategy).
    #[inline(always)]
    pub const fn ceil(self) -> Self {
        // ceil(x) = -floor(-x). Wrapping negation maps NaN to itself and every
        // finite raw to its exact opposite, so this inherits floor's handling
        let floor = Self(self.0.wrapping_neg()).floor();
        Self(floor.0.wrapping_neg())
    }

    /// The whole part, rounding towards zero. Always finite for a finite
    /// input; [`Dec::NAN`] stays NaN.
    ///
    /// The `dp` 0 case of
    /// [`RoundingStrategy::ToZero`](crate::RoundingStrategy).
    #[inline(always)]
    pub const fn trunc(self) -> Self {
        match self.is_nan() {
            true => Self::NAN,
            // the magnitude only shrinks, so the product cannot overflow
            false => Self((self.0 / ONE_RAW) * ONE_RAW),
        }
    }

    /// The sum, or `None` if it leaves the finite range or either side is
    /// [`Dec::NAN`]. Use this where an overflow should be handled on the spot
    /// rather than propagated.
    #[inline(always)]
    pub const fn checked_add(self, rhs: Self) -> Option<Self> {
        if self.is_nan() || rhs.is_nan() {
            return None;
        }
        match self.0.checked_add(rhs.0) {
            Some(raw) => match check_raw(raw) {
                Some(raw) => Some(Self(raw)),
                None => None,
            },
            None => None,
        }
    }

    /// The difference, or `None` if it leaves the finite range or either side
    /// is [`Dec::NAN`].
    #[inline(always)]
    pub const fn checked_sub(self, rhs: Self) -> Option<Self> {
        if self.is_nan() || rhs.is_nan() {
            return None;
        }
        match self.0.checked_sub(rhs.0) {
            Some(raw) => match check_raw(raw) {
                Some(raw) => Some(Self(raw)),
                None => None,
            },
            None => None,
        }
    }

    /// The product, or `None` if it leaves the finite range or either side is
    /// [`Dec::NAN`]. Exact, with the excess below [`Dec::SCALE`] rounded half
    /// away from zero.
    #[inline(always)]
    pub fn checked_mul(self, rhs: Self) -> Option<Self> {
        mul_raw(self.0, rhs.0).map(Self)
    }

    /// The quotient, or `None` on division by zero, if it leaves the finite
    /// range, or if either side is [`Dec::NAN`].
    #[inline(always)]
    pub fn checked_div(self, rhs: Self) -> Option<Self> {
        div_raw(self.0, rhs.0).map(Self)
    }

    /// The quotient, clamped to [`Dec::MIN`] or [`Dec::MAX`] on overflow.
    /// Division by zero has no side to clamp towards and still gives
    /// [`Dec::NAN`], as does a NaN operand.
    #[inline(always)]
    pub fn saturating_div(self, rhs: Self) -> Self {
        if self.is_nan() || rhs.is_nan() || rhs.is_zero() {
            return Self::NAN;
        }
        match self.checked_div(rhs) {
            Some(value) => value,
            None => match (self.0 < 0) != (rhs.0 < 0) {
                true => Self::MIN,
                false => Self::MAX,
            },
        }
    }

    /// The product, clamped to [`Dec::MIN`] or [`Dec::MAX`] on overflow.
    /// A [`Dec::NAN`] operand still gives NaN: there is no sign to clamp
    /// towards, and clamping an unknown would invent one.
    #[inline(always)]
    pub fn saturating_mul(self, rhs: Self) -> Self {
        if self.is_nan() || rhs.is_nan() {
            return Self::NAN;
        }
        match self.checked_mul(rhs) {
            Some(value) => value,
            None => match (self.0 < 0) != (rhs.0 < 0) {
                true => Self::MIN,
                false => Self::MAX,
            },
        }
    }

    /// The sum, clamped to [`Dec::MIN`] or [`Dec::MAX`] on overflow. A
    /// [`Dec::NAN`] operand still gives NaN.
    #[inline(always)]
    pub const fn saturating_add(self, rhs: Self) -> Self {
        match self.is_nan() || rhs.is_nan() {
            true => Self::NAN,
            false => Self(clamp_raw(self.0.saturating_add(rhs.0))),
        }
    }

    /// The difference, clamped to [`Dec::MIN`] or [`Dec::MAX`] on overflow. A
    /// [`Dec::NAN`] operand still gives NaN.
    #[inline(always)]
    pub const fn saturating_sub(self, rhs: Self) -> Self {
        match self.is_nan() || rhs.is_nan() {
            true => Self::NAN,
            false => Self(clamp_raw(self.0.saturating_sub(rhs.0))),
        }
    }

    // cannot overflow: the sum is formed in a wider space than either operand.
    // A half unit in the last place rounds towards zero rather than away from
    // it, the one place this type does not round halves away from zero.
    /// The value halfway between the two, which cannot overflow because the
    /// sum is formed in a wider space. [`Dec::NAN`] on either side gives NaN.
    #[inline(always)]
    pub const fn midpoint(self, rhs: Self) -> Self {
        match self.is_nan() || rhs.is_nan() {
            true => Self::NAN,
            false => Self(self.0.midpoint(rhs.0)),
        }
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use crate::Dec;
    use crate::dec;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::str::FromStr;

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
    fn test_extremes_that_stay_in_range_are_exact() {
        assert_eq!(Dec::MIN.abs(), Dec::MAX);
        assert_eq!(-Dec::MIN, Dec::MAX);
        // rounding towards zero cannot leave the range
        assert_eq!(Dec::MIN.ceil(), Dec::MIN.trunc());
        assert_eq!(Dec::MAX.floor(), Dec::MAX.trunc());
    }

    // the whole unit past either extreme is outside the finite range, and
    // saturating back onto it would report a value a full unit from the truth
    #[test]
    fn test_extremes_that_leave_the_range_become_nan() {
        assert!(Dec::MIN.floor().is_nan());
        assert!(Dec::MAX.ceil().is_nan());
        assert!(Dec::MIN.round_dp(0).is_nan());
        assert!(Dec::MAX.round_dp(0).is_nan());
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
            Dec::from_raw(1).midpoint(Dec::from_raw(2)),
            Dec::from_raw(1)
        );
        assert_eq!(
            Dec::from_raw(-1).midpoint(Dec::from_raw(-2)),
            Dec::from_raw(-1)
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
            worst = worst.max(ulps_from_truth(Dec::from_raw(raw)));
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
