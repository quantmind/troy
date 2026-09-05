use super::core::{Dec, ONE_U};

const CHUNK: u128 = 1_000_000_000;

// raw * 10^18 fits a u128 at or below this, which is every value up to about
// 340: sizes, rates and yields, but few prices.
const SIMPLE_MAX: u128 = u128::MAX / ONE_U;

impl Dec {
    /// The square root, rounded half away from zero at [`Dec::SCALE`] decimal
    /// places. A negative value gives [`Dec::NAN`], as does NaN itself.
    ///
    /// Alone among the operations here it cannot overflow. The largest root
    /// the range holds is about `1.3e10`, eight orders of magnitude inside it,
    /// so every non-negative finite value has a finite root and there is
    /// nothing to saturate towards - hence no `saturating_sqrt`.
    ///
    /// [`Dec::sqrt_approx`] is about three times faster and gives up the last
    /// places to get there.
    ///
    /// ```
    /// use troy::{Dec, dec};
    ///
    /// assert_eq!(dec!(16).sqrt(), dec!(4));
    /// assert_eq!(dec!(2).sqrt(), dec!(1.414213562373095049));
    /// assert!(dec!(-1).sqrt().is_nan());
    /// assert!(Dec::MAX.sqrt().is_finite());
    /// ```
    #[inline(always)]
    pub const fn sqrt(self) -> Self {
        match self.checked_sqrt() {
            Some(value) => value,
            None => Self::NAN,
        }
    }

    /// The square root, or `None` for a negative value or [`Dec::NAN`]. Those
    /// are its only failures: a root never leaves the finite range.
    ///
    /// ```
    /// use troy::{Dec, dec};
    ///
    /// assert_eq!(dec!(2.25).checked_sqrt(), Some(dec!(1.5)));
    /// assert_eq!(dec!(-1).checked_sqrt(), None);
    /// assert_eq!(Dec::NAN.checked_sqrt(), None);
    /// ```
    #[inline(always)]
    pub const fn checked_sqrt(self) -> Option<Self> {
        // NAN_RAW is the most negative raw, so one test takes it and every
        // negative value, which answer the same way in any case
        if self.0 < 0 {
            return None;
        }
        Some(Self(sqrt_raw(self.0 as u128)))
    }

    /// The square root through `f64`: about three times faster than
    /// [`Dec::sqrt`], and accurate to the double's sixteen significant digits
    /// rather than to the last place.
    ///
    /// Where [`Dec::sqrt`] is out by at most half a unit in the last place,
    /// this is out by a share of the root, so the error grows with it: at
    /// price scale the last four decimals are noise, at the top of the range
    /// the last twelve. It is not correctly rounded, and neighbouring values
    /// collapse onto the same root, so compare and reconcile on the exact one.
    ///
    /// Reach for it where the input is already uncertain and the answer feeds
    /// a float anyway: a volatility, a standard deviation, a Sharpe ratio.
    /// Reach for [`Dec::sqrt`] where the answer has to reconcile, compare
    /// equal, or go back into exact arithmetic.
    ///
    /// ```
    /// use troy::{Dec, dec};
    ///
    /// assert_eq!(dec!(16).sqrt_approx(), dec!(4));
    /// assert!(dec!(-1).sqrt_approx().is_nan());
    ///
    /// // the two agree to sixteen digits, and part after them
    /// let value = dec!(8237.41);
    /// let drift = (value.sqrt_approx() - value.sqrt()).abs();
    /// assert!(drift > Dec::ZERO && drift < dec!(0.0000000000001));
    /// ```
    #[inline]
    pub fn sqrt_approx(self) -> Self {
        // taken here rather than left to the conversions: `to_f64` would hand
        // a negative value to `f64::sqrt` and get an IEEE NaN back, which is a
        // different NaN arriving by a different route
        if self.0 < 0 {
            return Self::NAN;
        }
        match Self::from_f64(self.to_f64().sqrt()) {
            Some(value) => value,
            // unreachable: a root is at most 1.3e10, and only a NaN or a value
            // past the range is refused. Answering NaN rather than asserting
            // keeps the method total on a hot path.
            None => Self::NAN,
        }
    }
}

// The root of `raw / 10^18` at scale 10^18 is `sqrt(raw * 10^18)`, and 10^18
// is itself a perfect square. That is what makes this tractable: the answer is
// `10^9 * sqrt(raw)`, so the whole problem is `sqrt(raw)` carried nine digits
// further than its integer part.
//
// The product under the root needs 187 bits, which no u128 holds. Neither path
// forms it: the first because a small enough `raw` keeps it inside one, the
// second because it never needs the product, only comparisons against it.
//
// Rounding is half away from zero to match the rest of the crate, but no tie
// can arise to exercise it. A root sits exactly half a unit past an integer
// only when `raw * 10^18` equals `root^2 + root + 1/4`, and no integer is a
// quarter past another - so the test is simply `raw * 10^18 >= root^2 + root + 1`.
#[inline]
const fn sqrt_raw(raw: u128) -> i128 {
    match raw <= SIMPLE_MAX {
        true => simple(raw),
        false => wide(raw),
    }
}

// The whole product fits, so the standard library's integer root answers it
// directly and the rounding test is a subtraction.
#[inline(always)]
const fn simple(raw: u128) -> i128 {
    let scaled = raw * ONE_U;
    let root = scaled.isqrt();
    match scaled - root * root > root {
        true => root as i128 + 1,
        false => root as i128,
    }
}

// Past the threshold the product no longer fits, so the root is built out of
// `sqrt(raw)` instead: `whole` is its integer part, `excess` what is left over,
// and one Newton term carries that remainder the nine digits the scale wants.
//
// The threshold does double duty. It is where the product stops fitting a
// u128, and it is far above where the Newton term stops being tight: the term
// it drops is under `10^9 / (2 * whole)`, so past a `raw` of 3.4e20 - where
// `whole` exceeds 1.8e10 - the estimate is within 0.03 of the true root and
// can only be one out through the flooring. Below the threshold that bound
// falls apart (at `raw` of 2 the estimate is 81 million out), which is why the
// two paths cannot simply be reordered.
//
// So the loops are the proof, not the work: over 800k sampled raws neither ran
// once. They cost one comparison each and, in exchange, the answer does not
// rest on the error bound holding at the edges.
const fn wide(raw: u128) -> i128 {
    let whole = raw.isqrt();
    let excess = raw - whole * whole;
    // excess < 2 * whole + 1, so the Newton term stays under 10^9 and the sum
    // under the 1.3e28 the largest root reaches
    let mut root = whole * CHUNK + excess * CHUNK / (2 * whole + 1);

    while !square_at_most(root, 0, raw) {
        root -= 1;
    }
    while square_at_most(root + 1, 0, raw) {
        root += 1;
    }

    match square_at_most(root, root + 1, raw) {
        true => root as i128 + 1,
        false => root as i128,
    }
}

// `root^2 + bias <= raw * 10^18`, decided inside a u128 because neither side
// is ever formed. Splitting the root at 10^9 into `high` and `low` gives
//
//     root^2 = high^2 * 10^18 + 2 * high * low * 10^9 + low^2
//
// whose leading term cancels against the right side, and what is left fits:
// the largest root this is asked about takes the left to 2.6e37, an order of
// magnitude inside a u128, and the right is capped below.
#[inline(always)]
const fn square_at_most(root: u128, bias: u128, raw: u128) -> bool {
    let (high, low) = (root / CHUNK, root % CHUNK);
    match raw.checked_sub(high * high) {
        // the leading term alone is already past the right side
        None => false,
        Some(slack) => match slack.checked_mul(ONE_U) {
            Some(limit) => 2 * high * low * CHUNK + low * low + bias <= limit,
            // a slack past 3.4e20 puts the right side beyond anything the left
            // reaches, so the comparison is settled without the product
            None => true,
        },
    }
}

#[cfg(test)]
mod root {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::dec;
    use std::str::FromStr;

    fn parse(text: &str) -> Dec {
        Dec::from_str(text).unwrap()
    }

    #[test]
    fn test_the_threshold_splits_the_paths_where_the_product_stops_fitting() {
        // one below it the product still fits a u128, one above it does not
        assert!(SIMPLE_MAX.checked_mul(ONE_U).is_some());
        assert_eq!((SIMPLE_MAX + 1).checked_mul(ONE_U), None);
        // which puts the split at about 340, so sizes take the simple path
        // and prices generally do not
        assert!(dec!(100).into_raw() as u128 <= SIMPLE_MAX);
        assert!(dec!(104_237.25).into_raw() as u128 > SIMPLE_MAX);
    }

    #[test]
    fn test_the_two_paths_agree_where_both_are_valid() {
        // the wide path's estimate is only tight once `whole` is large, so the
        // overlap runs from a raw of 10^18 - a value of 1, where the dropped
        // term is already under half a unit - up to the threshold
        let mut state = 0x2545_F491_4F6C_DD1D_u64;
        for _ in 0..20_000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let raw = ONE_U + (state as u128) % (SIMPLE_MAX - ONE_U);
            assert_eq!(simple(raw), wide(raw), "{raw}");
        }
        for raw in [ONE_U, ONE_U + 1, SIMPLE_MAX - 1, SIMPLE_MAX] {
            assert_eq!(simple(raw), wide(raw), "{raw}");
        }
    }

    #[test]
    fn test_a_perfect_square_is_exact() {
        for text in ["0", "1", "4", "16", "100", "2.25", "0.25", "0.000000000001"] {
            let value = parse(text);
            assert_eq!(value.sqrt() * value.sqrt(), value, "{text}");
        }
        assert_eq!(dec!(16).sqrt(), dec!(4));
        assert_eq!(dec!(2.25).sqrt(), dec!(1.5));
        assert_eq!(Dec::ZERO.sqrt(), Dec::ZERO);
        assert_eq!(Dec::ONE.sqrt(), Dec::ONE);
    }

    #[test]
    fn test_an_irrational_root_rounds_half_away_from_zero() {
        assert_eq!(dec!(2).sqrt(), parse("1.414213562373095049"));
        assert_eq!(dec!(3).sqrt(), parse("1.732050807568877294"));
        assert_eq!(dec!(10).sqrt(), parse("3.162277660168379332"));
    }

    #[test]
    fn test_the_root_brackets_the_value() {
        // the defining property, checked on the raws either side: the root
        // squares to at most the value, and one unit more does not
        let mut state = 0x1234_5678_9abc_def1_u64;
        for _ in 0..20_000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let raw = (state as u128) * (state as u128 | 1) % (Dec::MAX.into_raw() as u128);
            let root = sqrt_raw(raw) as u128;
            let floor = match square_at_most(root, 0, raw) {
                true => root,
                // rounded up, so the floor is one below
                false => root - 1,
            };
            assert!(square_at_most(floor, 0, raw), "{raw}");
            assert!(!square_at_most(floor + 1, 0, raw), "{raw}");
        }
    }

    #[test]
    fn test_the_root_is_monotonic() {
        let mut previous = Dec::ZERO;
        for raw in (0..2_000).map(|step: u32| step as i128 * (Dec::MAX.into_raw() / 2_000)) {
            let root = Dec::from_raw(raw).sqrt();
            assert!(root >= previous, "{raw}");
            previous = root;
        }
    }

    #[test]
    fn test_the_largest_value_has_a_root_far_inside_the_range() {
        let root = Dec::MAX.sqrt();
        assert!(root.is_finite());
        assert_eq!(root, parse("13043817825.332782212349571806"));
        // and it squares back to within a rounding of where it came from
        assert!(root * root <= Dec::MAX);
    }

    #[test]
    fn test_the_smallest_value_has_a_root() {
        // 10^-18 is a perfect square at this scale, its root the 10^-9 that
        // both paths are built around
        assert_eq!(Dec::EPSILON.sqrt(), parse("0.000000001"));
        assert_eq!(Dec::from_raw(2).sqrt(), parse("0.000000001414213562"));
    }

    #[test]
    fn test_the_approximate_root_keeps_the_double_s_digits() {
        // the drift is a share of the root rather than a fixed number of
        // units, so the bound has to scale with it; the constant is a few
        // times the double's own 2.2e-16, and the floor covers the roots small
        // enough that eighteen places are the coarser of the two
        let mut state = 0x5eed_1234_abcd_0f0f_u64;
        for _ in 0..50_000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let wide = (state as u128) << 64 | state.rotate_left(29) as u128;
            let value = Dec::from_raw((wide % (Dec::MAX.into_raw() as u128)) as i128);
            let (exact, approx) = (value.sqrt(), value.sqrt_approx());
            let drift = (approx - exact).abs();
            let allowed = exact * parse("0.000000000000001") + parse("0.000000000000001");
            assert!(
                drift <= allowed,
                "sqrt({value}) drifted {drift}, allowed {allowed}"
            );
        }
    }

    #[test]
    fn test_the_approximate_root_is_exact_where_the_double_is() {
        // a perfect square small enough to be a double exactly comes back
        // exactly, which is what makes the method usable at all
        for text in ["0", "1", "4", "16", "100", "2.25", "0.25", "0.0625"] {
            let value = parse(text);
            assert_eq!(value.sqrt_approx(), value.sqrt(), "{text}");
        }
    }

    #[test]
    fn test_both_roots_answer_nan_on_the_same_inputs() {
        for value in [dec!(-1), Dec::MIN, Dec::NAN, dec!(-0.000000000000000001)] {
            assert!(value.sqrt().is_nan(), "{value}");
            assert!(value.sqrt_approx().is_nan(), "{value}");
        }
    }

    #[test]
    fn test_a_negative_value_is_nan_rather_than_a_complex_number() {
        assert!(dec!(-1).sqrt().is_nan());
        assert!(dec!(-0.000000000000000001).sqrt().is_nan());
        assert!(Dec::MIN.sqrt().is_nan());
        assert_eq!(dec!(-1).checked_sqrt(), None);
        assert_eq!(Dec::MIN.checked_sqrt(), None);
    }

    #[test]
    fn test_nan_propagates_through_the_root() {
        assert!(Dec::NAN.sqrt().is_nan());
        assert_eq!(Dec::NAN.checked_sqrt(), None);
    }

    #[test]
    fn test_the_root_is_available_in_a_const() {
        const ROOT: Dec = dec!(6.25).sqrt();
        assert_eq!(ROOT, dec!(2.5));
    }
}
