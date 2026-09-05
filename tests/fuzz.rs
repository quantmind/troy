//! Property tests: the invariants that hold for every input, checked against
//! inputs proptest chooses rather than inputs a person thought of.
//!
//! [`tests/oracle.rs`](../oracle.rs) covers multiplication and division against
//! an exact 256-bit reference. This file covers everything else: the parser,
//! the renderer, rounding, the `f64` conversions and the operator contracts.
//!
//! Each property is stated against something other than a second copy of the
//! implementation, because a test that reimplements the code it checks agrees
//! with it for the same wrong reason:
//!
//! - the parser is checked against the renderer, and against the several
//!   equivalent spellings of one value that must all land on it;
//! - the renderer is checked by round trip, and by the shape of what it emits;
//! - `round_to_step` is checked against `round_dp` on every power of ten, and
//!   against the definition of a multiple on every other step;
//! - `to_f64` is checked against the standard library's float parser, which is
//!   correctly rounded and shares no code with anything here;
//! - the operators are checked against their `checked_` counterparts, which is
//!   the one place two implementations of the same thing are meant to agree.
//!
//! The case count comes from `PROPTEST_CASES` when it is set, so `make rs-fuzz`
//! can run a far wider sweep in release than `cargo test` runs in debug.

use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use std::str::FromStr;
use troy::{Dec, ParseDecError};

/// The crate root is `src/mod.rs` rather than `src/lib.rs`, so proptest cannot
/// locate it to place a regression file and warns on every run unless told
/// where to record a failing case. `PROPTEST_CASES` still overrides the count.
fn config() -> ProptestConfig {
    ProptestConfig {
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "tests/fuzz.proptest-regressions",
        ))),
        ..ProptestConfig::default()
    }
}

/// The finite range is symmetric and `i128::MIN` is held out of it as the NaN
/// pattern, so this is every value `Dec` can represent.
const MIN_RAW: i128 = Dec::MIN.into_raw();
const MAX_RAW: i128 = Dec::MAX.into_raw();

/// One unit at the native scale.
const ONE_RAW: i128 = 1_000_000_000_000_000_000;

/// Half the range, which is where a property that rounds a value up needs to
/// stay: rounding at the very top of the range leaves it, which is its own
/// documented answer rather than a case worth generating.
const HALF_MAX_RAW: i128 = MAX_RAW / 2;

// -- strategies --------------------------------------------------------------

/// Any finite value, uniform across the whole range. Most draws are enormous,
/// which is the point: it is the far end of the range where the renderer's
/// 21-digit path and the parser's promotion to `u128` live.
fn finite() -> impl Strategy<Value = Dec> {
    (MIN_RAW..=MAX_RAW).prop_map(Dec::from_raw)
}

/// Any value including [`Dec::NAN`], which appears about once in fifty draws.
fn any_dec() -> impl Strategy<Value = Dec> {
    prop_oneof![
        49 => finite(),
        1 => Just(Dec::NAN),
    ]
}

/// Values within roughly +/-9.2e9, the range where a price, a size or a
/// notional actually lives, and the range `to_f64` has a fast path for.
fn modest() -> impl Strategy<Value = Dec> {
    (-9_000_000_000_000_000_000_i128..=9_000_000_000_000_000_000).prop_map(Dec::from_raw)
}

/// Finite doubles across the magnitudes the conversion has to cope with: the
/// range a price or size lives in, the small end where eighteen decimal places
/// runs out before the double does, and the top of what the type holds.
///
/// Reinterpreting random bits as a double would be the obvious thing and is the
/// wrong one: almost every bit pattern is an infinity, a NaN or something past
/// 10^18, so the strategy spends its budget on inputs the conversion rejects
/// before it reaches the arithmetic worth checking.
fn reasonable_f64() -> impl Strategy<Value = f64> {
    prop_oneof![
        3 => -1e9_f64..1e9,
        2 => -1.0_f64..1.0,
        2 => -1e-6_f64..1e-6,
        1 => -1e18_f64..1e18,
    ]
}

/// Text shaped like a decimal without being guaranteed to be one: the
/// characters the parser gives meaning to, assembled at random. Purely random
/// bytes almost never reach past the first `InvalidDigit`, so they exercise the
/// rejection path and nothing else; this reaches the accumulator, the
/// promotion, the exponent and the scale shift.
fn decimal_shaped_text() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        prop_oneof![
            10 => prop::char::range('0', '9'),
            2 => Just('.'),
            2 => Just('_'),
            1 => Just('e'),
            1 => Just('E'),
            1 => Just('-'),
            1 => Just('+'),
            1 => Just(' '),
        ],
        0..40,
    )
    .prop_map(|chars| chars.into_iter().collect())
}

// -- the parser --------------------------------------------------------------

/// The several ways one value may be spelled. All of them must parse to it.
///
/// Every form is derived from the raw integer rather than from a second parse,
/// so the expected answer is exact by construction: `raw * 10^-18` is the
/// definition of the value, and each spelling below is that identity written
/// differently.
fn equivalent_spellings(raw: i128) -> Vec<String> {
    let sign = if raw < 0 { "-" } else { "" };
    let digits = raw.unsigned_abs().to_string();
    let mut forms = Vec::new();

    // the point placed every distance from the right, with the exponent making
    // up the difference: 1.5 as "1500...0e-18", "15000...0.0e-16" and so on
    for point in 0..=Dec::SCALE as usize {
        let padded = format!("{digits:0>width$}", width = point + 1);
        let split = padded.len() - point;
        let exponent = Dec::SCALE as i32 - point as i32;
        forms.push(format!(
            "{sign}{}.{}e-{exponent}",
            &padded[..split],
            &padded[split..]
        ));
    }

    // the same value with the exponent capitalised, with a leading plus, with
    // leading zeros, and with the separator the parser is documented to ignore
    let plain = Dec::from_raw(raw).to_string();
    forms.push(plain.replace('e', "E"));
    forms.push(format!("{sign}000{}", plain.trim_start_matches('-')));
    forms.push(format!("  {plain}"));
    if raw > 0 {
        forms.push(format!("+{plain}"));
    }
    if let Some(point) = plain.find('.') {
        // trailing zeros on the fraction, up to but not past the native scale
        let spare = Dec::SCALE as usize - (plain.len() - point - 1);
        forms.push(format!("{plain}{}", "0".repeat(spare)));
    }
    forms
}

proptest! {
    #![proptest_config(config())]

    /// Arbitrary bytes must be rejected, not crash. This is the surface that
    /// takes untrusted text off an exchange feed, so the absence of a panic is
    /// the whole assertion.
    #[test]
    fn test_parsing_arbitrary_bytes_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..64)) {
        let text = String::from_utf8_lossy(&bytes);
        let _ = Dec::from_str(&text);
        let _ = Dec::parse_const(&text);
    }

    /// The same, for text made only of characters the parser gives meaning to,
    /// which reaches far deeper into it than random bytes do.
    #[test]
    fn test_parsing_decimal_shaped_text_never_panics(text in decimal_shaped_text()) {
        let _ = Dec::from_str(&text);
    }

    /// Whatever the parser accepts, it accepts into the finite range: it must
    /// never manufacture a NaN, which is reserved for a computation that
    /// overflowed and has no meaning coming out of text.
    #[test]
    fn test_a_parsed_value_is_never_nan(text in decimal_shaped_text()) {
        if let Ok(value) = Dec::from_str(&text) {
            prop_assert!(value.is_finite(), "{text:?} parsed to NaN");
        }
    }

    /// `parse_const` backs the `dec!` macro, so a literal that compiles and a
    /// string that parses must be the same value. A divergence would mean a
    /// constant in the source reading differently from the same text on the
    /// wire.
    #[test]
    fn test_the_const_parser_agrees_with_from_str(text in decimal_shaped_text()) {
        prop_assert_eq!(Dec::parse_const(&text), Dec::from_str(&text).ok());
    }

    /// Every spelling of a value parses to that value.
    #[test]
    fn test_equivalent_spellings_parse_to_the_same_value(raw in MIN_RAW..=MAX_RAW) {
        let expected = Dec::from_raw(raw);
        for form in equivalent_spellings(raw) {
            prop_assert_eq!(
                Dec::from_str(&form),
                Ok(expected),
                "spelling {:?} of {}",
                form,
                expected
            );
        }
    }

    /// Text past the finite range is rejected as `Overflow`, not wrapped into a
    /// plausible-looking value of the wrong sign.
    #[test]
    fn test_text_past_the_range_overflows(excess in 1_i8..=127) {
        for (limit, sign) in [(MAX_RAW, ""), (MIN_RAW, "-")] {
            // one digit wider than the widest value the range holds
            let text = format!("{sign}{}{}", limit.unsigned_abs(), excess % 10);
            prop_assert_eq!(
                Dec::from_str(&text),
                Err(ParseDecError::Overflow),
                "{}",
                text
            );
        }
    }

    /// Digits below the native scale round half away from zero rather than
    /// truncating.
    ///
    /// The fraction is padded to the full scale before the excess is appended,
    /// so the excess genuinely sits below the last place the type holds. Append
    /// to the canonical rendering instead and its stripped trailing zeros shift
    /// the tail up into places that are represented, which is a different
    /// question with a different answer.
    #[test]
    fn test_excess_precision_rounds_half_away_from_zero(
        // held to half the range so that rounding up cannot leave it. The
        // text itself is unbounded in width: near the top of the range it runs
        // to 41 significant digits, past what the mantissa holds, which is the
        // case worth reaching
        raw in -HALF_MAX_RAW..=HALF_MAX_RAW,
        tail in 0_u32..1_000,
    ) {
        let magnitude = raw.unsigned_abs();
        let sign = if raw < 0 { "-" } else { "" };
        let text = format!(
            "{sign}{}.{:018}{tail:03}",
            magnitude / ONE_RAW as u128,
            magnitude % ONE_RAW as u128,
        );
        let parsed = Dec::from_str(&text).expect("in range, with three digits added");
        // a tie at exactly half a unit carries away from zero, and the excess
        // can move the value by no more than that one unit either way
        let expected = match (tail >= 500, raw < 0) {
            (true, true) => -1,
            (true, false) => 1,
            (false, _) => 0,
        };
        prop_assert_eq!(
            parsed.into_raw() - raw,
            expected,
            "{} parsed to {}",
            text,
            parsed
        );
    }
}

// -- the renderer ------------------------------------------------------------

proptest! {
    #![proptest_config(config())]

    /// The round trip that matters: whatever is rendered parses back to what
    /// was rendered. A renderer that dropped a digit, and a parser that
    /// misread one, would both show up here.
    #[test]
    fn test_every_finite_value_round_trips_through_its_text(value in finite()) {
        let text = value.to_string();
        prop_assert_eq!(Dec::from_str(&text), Ok(value), "text {}", text);
    }

    /// The rendering is canonical: one value has exactly one spelling. No
    /// trailing zero in the fraction, no bare point, no leading zero on the
    /// integer part, and a sign only when the value is negative.
    #[test]
    fn test_the_rendering_is_canonical(value in finite()) {
        let text = value.to_string();
        prop_assert!(!text.ends_with('.'), "{text} ends with a bare point");
        if let Some((integer, fraction)) = text.split_once('.') {
            prop_assert!(!fraction.ends_with('0'), "{text} has a trailing zero");
            prop_assert!(!fraction.is_empty(), "{text} has an empty fraction");
            prop_assert!(
                fraction.len() <= Dec::SCALE as usize,
                "{text} carries more than the native scale"
            );
            prop_assert!(!integer.is_empty(), "{text} has no integer part");
        }
        let magnitude = text.trim_start_matches('-');
        prop_assert!(
            !magnitude.starts_with('0') || magnitude.starts_with("0."),
            "{text} has a leading zero"
        );
        prop_assert_eq!(text.starts_with('-'), value.is_sign_negative());
    }

    /// A precision emits exactly the places asked for: no stripping, no bare
    /// point, and the sign still only on a negative value.
    #[test]
    fn test_a_precision_emits_exactly_the_places_asked_for(
        value in finite(),
        dp in 0_usize..=24,
    ) {
        let text = format!("{value:.dp$}");
        match text.split_once('.') {
            Some((integer, fraction)) => {
                prop_assert_ne!(dp, 0, "{} carries a point at zero places", text);
                prop_assert_eq!(fraction.len(), dp, "{} is not {} places", text, dp);
                prop_assert!(fraction.bytes().all(|byte| byte.is_ascii_digit()), "{}", text);
                prop_assert!(!integer.is_empty(), "{} has no integer part", text);
            }
            None => prop_assert_eq!(dp, 0, "{} has no point at {} places", text, dp),
        }
        prop_assert_eq!(text.starts_with('-'), value.is_sign_negative(), "{}", text);
    }

    /// A precision rounds the way `round_dp` does. The two share no code: the
    /// renderer rounds the magnitude as a `u128` on its way to digits, where
    /// `round_dp` rounds the signed raw and rebuilds a value from it.
    #[test]
    fn test_a_precision_rounds_the_way_round_dp_does(
        value in finite(),
        dp in 0_u32..=Dec::SCALE,
    ) {
        let rounded = value.round_dp(dp);
        // the top of the range rounds out of it, which the renderer spells out
        // and `round_dp` answers with NaN
        prop_assume!(rounded.is_finite());
        let text = format!("{value:.width$}", width = dp as usize);
        prop_assert_eq!(Dec::from_str(&text), Ok(rounded), "text {}", text);
    }

    /// The rendering orders the same way the value does, once the sign is
    /// accounted for. A value whose text sorts differently from itself would
    /// break every log, key and comparison built on the string form.
    #[test]
    fn test_rendering_preserves_order_within_a_sign(a in finite(), b in finite()) {
        let (a, b) = (a.abs(), b.abs());
        let (left, right) = (a.to_string(), b.to_string());
        // longer integer parts are larger; equal lengths compare lexically
        let key = |text: &str| {
            let integer = text.split_once('.').map_or(text.len(), |(head, _)| head.len());
            (integer, text.to_string())
        };
        prop_assert_eq!(a.cmp(&b), key(&left).cmp(&key(&right)), "{} vs {}", left, right);
    }
}

/// Text carrying more significant digits than a `u128` holds is rounded to the
/// scale, not rejected.
///
/// The parser accumulates significant digits and applies the scale shift
/// afterwards, so a full mantissa used to fail the parse with `Overflow` even
/// where the value named sat well inside the range: `1.0` written with 43
/// digits, or `Dec::MAX` with one digit of excess. `Overflow` is the error
/// meaning the value is too large, and a caller acting on it would have dropped
/// a price that was merely written too precisely.
///
/// A full mantissa now stops the accumulation instead. Every digit past it is
/// below the last one kept, so it rounds that one and then only moves the
/// exponent.
#[test]
fn test_precision_past_the_mantissa_rounds_rather_than_failing() {
    // one, spelled with 43 significant digits
    let one = format!("1.{}1", "0".repeat(41));
    assert_eq!(Dec::from_str(&one), Ok(Dec::ONE));

    // the widest value the type holds, with a digit of excess either side of
    // the half that decides the rounding
    assert_eq!(Dec::from_str(&format!("{}1", Dec::MAX)), Ok(Dec::MAX));
    assert_eq!(Dec::from_str(&format!("{}4", Dec::MIN)), Ok(Dec::MIN));
    assert_eq!(
        Dec::from_str(&format!("{}5", Dec::MAX)),
        Err(ParseDecError::Overflow),
        "rounding up here really does leave the range"
    );

    // a value genuinely past the range is still rejected, and for the right
    // reason: the digits are wide because the value is large
    assert_eq!(
        Dec::from_str("1701411834604692317316873037158841057270"),
        Err(ParseDecError::Overflow)
    );

    // a small value keeps its excess precision, as it always did: its mantissa
    // stays narrow however many leading zeros the fraction carries
    let tiny = format!("0.{}1", "0".repeat(41));
    assert_eq!(Dec::from_str(&tiny), Ok(Dec::ZERO));
}

/// NaN renders as `NaN` and does not parse back, which is deliberate: the text
/// form is for a person reading a log, and a fault must not be able to
/// re-enter the system as a value by way of its own rendering.
#[test]
fn test_nan_renders_but_does_not_round_trip() {
    assert_eq!(Dec::NAN.to_string(), "NaN");
    assert_eq!(format!("{:?}", Dec::NAN), "NaN");
    assert_eq!(Dec::from_str("NaN"), Err(ParseDecError::InvalidDigit));
    assert_eq!(Dec::parse_const("NaN"), None);
}

// -- rounding ----------------------------------------------------------------

proptest! {
    #![proptest_config(config())]

    /// `round_to_step` has a fast path for powers of ten that works on the
    /// fraction inside a `u64`, and a general path that divides. On a power of
    /// ten the two must reach what `round_dp` reaches, over the whole range
    /// rather than the handful of values the unit test names.
    #[test]
    fn test_round_to_step_matches_round_dp_on_every_power_of_ten(value in any_dec()) {
        for dp in 0..=Dec::SCALE {
            let step = Dec::from_raw(10_i128.pow(Dec::SCALE - dp));
            prop_assert_eq!(
                value.round_to_step(step),
                value.round_dp(dp),
                "value {} at {} places",
                value,
                dp
            );
        }
    }

    /// Rounding twice to the same place changes nothing the second time.
    #[test]
    fn test_rounding_is_idempotent(value in any_dec(), dp in 0_u32..=Dec::SCALE) {
        let once = value.round_dp(dp);
        prop_assert_eq!(once.round_dp(dp), once, "value {} at {} places", value, dp);
    }

    /// Rounding moves a value by at most half a step. This is what makes it
    /// rounding rather than truncation, and it is the property a wrong-signed
    /// correction breaks.
    #[test]
    fn test_rounding_moves_by_at_most_half_a_step(value in finite(), dp in 0_u32..=Dec::SCALE) {
        let rounded = value.round_dp(dp);
        // near the ends of the range rounding overflows, which is its own
        // documented answer and has no distance to measure
        prop_assume!(rounded.is_finite());
        let step = 10_i128.pow(Dec::SCALE - dp);
        let drift = rounded.into_raw() - value.into_raw();
        prop_assert!(
            drift.abs() * 2 <= step,
            "{value} rounded to {dp} places moved {drift}, past half of {step}"
        );
    }

    /// A rounded value is a multiple of the step it was rounded to, for an
    /// arbitrary step rather than only a power of ten.
    ///
    /// The value is held to half the range and the step to 10^24 so that
    /// rounding up cannot leave the range: drawn uniformly, a step is almost
    /// always astronomical, every case overflows to NaN, and the property ends
    /// up asserting nothing about the arithmetic it is meant to cover.
    #[test]
    fn test_a_rounded_value_is_a_multiple_of_its_step(
        raw in MIN_RAW / 2..=MAX_RAW / 2,
        step in 1_i128..=10_i128.pow(24),
    ) {
        let (value, step) = (Dec::from_raw(raw), Dec::from_raw(step));
        let rounded = value.round_to_step(step);
        prop_assert!(rounded.is_finite(), "{value} to a step of {step} overflowed");
        prop_assert_eq!(
            rounded.into_raw() % step.into_raw(),
            0,
            "{} is not a multiple of {}",
            rounded,
            step
        );
    }

    /// Rounding never changes the sign of a value, only its magnitude, and it
    /// never moves a value away from zero past the next step.
    #[test]
    fn test_rounding_never_flips_the_sign(value in finite(), dp in 0_u32..=Dec::SCALE) {
        let rounded = value.round_dp(dp);
        prop_assume!(rounded.is_finite());
        if !rounded.is_zero() {
            prop_assert_eq!(
                rounded.is_sign_negative(),
                value.is_sign_negative(),
                "{} rounded to {} at {} places",
                value,
                rounded,
                dp
            );
        }
    }

    /// `floor` and `ceil` bracket the value, and `trunc` sits between them.
    #[test]
    fn test_floor_and_ceil_bracket_the_value(value in modest()) {
        let (floor, ceil, trunc) = (value.floor(), value.ceil(), value.trunc());
        prop_assert!(floor <= value, "floor {floor} above {value}");
        prop_assert!(ceil >= value, "ceil {ceil} below {value}");
        prop_assert!(trunc >= floor && trunc <= ceil, "trunc {trunc} outside [{floor}, {ceil}]");
        for whole in [floor, ceil, trunc] {
            prop_assert_eq!(whole.into_raw() % ONE_RAW, 0, "{} is not whole", whole);
        }
        // they differ by exactly one unit unless the value was already whole
        let span = ceil.into_raw() - floor.into_raw();
        prop_assert_eq!(span, if value.into_raw() % ONE_RAW == 0 { 0 } else { ONE_RAW });
    }
}

// -- the f64 conversions -----------------------------------------------------

proptest! {
    #![proptest_config(config())]

    /// `to_f64` has a fast path for values that divide exactly by 10^9 and a
    /// fallback for the rest. Both must land on the same double the standard
    /// library's parser reaches from the same text, which is correctly rounded
    /// and shares no code with any of this.
    ///
    /// Two roundings on either path put the answer within an ulp of the
    /// correctly rounded one, so that is what is asserted rather than equality.
    #[test]
    fn test_to_f64_matches_the_standard_library(value in finite()) {
        let reference: f64 = value.to_string().parse().expect("a rendered decimal");
        let actual = value.to_f64();
        let ulps = (actual.to_bits() as i64).abs_diff(reference.to_bits() as i64);
        prop_assert!(
            ulps <= 1,
            "{value} converted to {actual:e}, {ulps} ulps from {reference:e}"
        );
    }

    /// The conversion is monotonic: it never reports one value as smaller than
    /// another it is larger than. A fast path disagreeing with the fallback
    /// would show up here even where both stay within an ulp.
    #[test]
    fn test_to_f64_is_monotonic(a in finite(), b in finite()) {
        if a < b {
            prop_assert!(a.to_f64() <= b.to_f64(), "{a} -> {:e} not below {b} -> {:e}", a.to_f64(), b.to_f64());
        }
    }

    /// A value that came from an `f64` converts back to it within the precision
    /// a fixed scale can hold.
    ///
    /// The bound is absolute, not relative, and that is the substance of the
    /// property rather than a slack tolerance. Two things set it: eighteen
    /// decimal places is all there is, so a value near 10^-9 keeps ten
    /// significant digits where one near 10 keeps seventeen; and
    /// `trim_f64_noise` deliberately drops the scaled fraction to fifteen
    /// digits, on the grounds that a double did not carry the rest. Together
    /// they cost at most ~5e-16, and the double's own ulp carries the rest at
    /// larger magnitudes.
    ///
    /// A caller pricing something near 10^-9 should read that as: this type
    /// will not hold your seventeenth digit, by construction.
    #[test]
    fn test_a_value_from_an_f64_returns_to_it(original in reasonable_f64()) {
        if let Some(value) = Dec::from_f64(original) {
            let back = value.to_f64();
            let tolerance = 5e-16 + original.abs() * 5e-16;
            prop_assert!(
                (back - original).abs() <= tolerance,
                "{original:e} became {value} and returned as {back:e}"
            );
        }
    }

    /// `from_f64` never yields NaN. A conversion reports failure with `None`,
    /// because there is no earlier computation for a NaN to have come from.
    ///
    /// This one does take raw bits: the infinities and the NaNs are exactly the
    /// inputs worth pushing at it, and none of them may come back as a value.
    #[test]
    fn test_from_f64_never_yields_nan(bits in any::<u64>()) {
        if let Some(value) = Dec::from_f64(f64::from_bits(bits)) {
            prop_assert!(value.is_finite(), "{:e} became NaN", f64::from_bits(bits));
        }
    }

    /// Pinning a float to a scale is stable: rounding it again at the same
    /// scale, by either route, changes nothing.
    #[test]
    fn test_from_f64_round_is_stable(original in reasonable_f64(), dp in 0_u32..=Dec::SCALE) {
        if let Some(value) = Dec::from_f64_round(original, dp) {
            prop_assert_eq!(value.round_dp(dp), value, "{:e} at {} places", original, dp);
        }
    }
}

// -- the operator contracts --------------------------------------------------

/// Every operator, paired with its `checked_` counterpart.
macro_rules! operator_pairs {
    ($a:expr, $b:expr, $body:expr) => {{
        let checked: [(&str, Dec, Option<Dec>); 4] = [
            ("add", $a + $b, $a.checked_add($b)),
            ("sub", $a - $b, $a.checked_sub($b)),
            ("mul", $a * $b, $a.checked_mul($b)),
            ("div", $a / $b, $a.checked_div($b)),
        ];
        #[allow(clippy::redundant_closure_call)]
        for (name, operator, checked) in checked {
            $body(name, operator, checked)?;
        }
    }};
}

proptest! {
    #![proptest_config(config())]

    /// An operator answers NaN exactly where its `checked_` counterpart answers
    /// `None`, and the same value everywhere else. The two are the same
    /// arithmetic with two ways of reporting a fault, so any divergence means
    /// one of them is lying about whether the result is trustworthy.
    #[test]
    fn test_operators_and_their_checked_counterparts_agree(a in any_dec(), b in any_dec()) {
        operator_pairs!(a, b, |name, operator: Dec, checked: Option<Dec>| {
            match checked {
                Some(value) => prop_assert_eq!(operator, value, "{} of {} and {}", name, a, b),
                None => prop_assert!(operator.is_nan(), "{} of {} and {} kept {}", name, a, b, operator),
            }
            Ok(())
        });
    }

    /// A NaN operand poisons every operation it reaches, so a fault cannot be
    /// laundered into a finite number by any route.
    #[test]
    fn test_nan_propagates_through_everything(value in finite(), dp in 0_u32..=Dec::SCALE) {
        for (left, right) in [(Dec::NAN, value), (value, Dec::NAN), (Dec::NAN, Dec::NAN)] {
            prop_assert!((left + right).is_nan(), "add kept {}", left + right);
            prop_assert!((left - right).is_nan(), "sub kept {}", left - right);
            prop_assert!((left * right).is_nan(), "mul kept {}", left * right);
            prop_assert!((left / right).is_nan(), "div kept {}", left / right);
            prop_assert!(left.saturating_add(right).is_nan(), "saturating_add kept a value");
            prop_assert!(left.saturating_sub(right).is_nan(), "saturating_sub kept a value");
            prop_assert!(left.saturating_mul(right).is_nan(), "saturating_mul kept a value");
            prop_assert!(left.saturating_div(right).is_nan(), "saturating_div kept a value");
            prop_assert!(left.midpoint(right).is_nan(), "midpoint kept a value");
            prop_assert!(left.round_to_step(right).is_nan(), "round_to_step kept a value");
        }
        for unary in [
            Dec::NAN.abs(),
            Dec::NAN.signum(),
            Dec::NAN.floor(),
            Dec::NAN.ceil(),
            Dec::NAN.trunc(),
            Dec::NAN.round_dp(dp),
            -Dec::NAN,
        ] {
            prop_assert!(unary.is_nan(), "a unary operation recovered {}", unary);
        }
        // and the predicates agree that it is nothing in particular
        prop_assert!(!Dec::NAN.is_zero());
        prop_assert!(!Dec::NAN.is_sign_negative());
        prop_assert!(!Dec::NAN.is_sign_positive());
        prop_assert!(!Dec::NAN.is_finite());
        prop_assert_eq!(Dec::NAN.to_f64().is_nan(), true);
    }

    /// Saturation clamps to an end of the range with the sign the exact answer
    /// would have had. It must never wrap, and never invent the opposite sign,
    /// which is the failure that would put a buy where a sell belonged.
    #[test]
    fn test_saturating_operations_clamp_rather_than_wrap(a in finite(), b in finite()) {
        let saturating: [(&str, Dec, Option<Dec>); 4] = [
            ("add", a.saturating_add(b), a.checked_add(b)),
            ("sub", a.saturating_sub(b), a.checked_sub(b)),
            ("mul", a.saturating_mul(b), a.checked_mul(b)),
            ("div", a.saturating_div(b), a.checked_div(b)),
        ];
        for (name, actual, exact) in saturating {
            // division by zero is the one saturation with no side to clamp to
            if name == "div" && b.is_zero() {
                prop_assert!(actual.is_nan(), "div by zero gave {actual}");
                continue;
            }
            prop_assert!(actual.is_finite(), "saturating_{name} of {a} and {b} gave NaN");
            match exact {
                Some(value) => prop_assert_eq!(actual, value, "saturating_{} of {} and {}", name, a, b),
                None => prop_assert!(
                    actual == Dec::MAX || actual == Dec::MIN,
                    "saturating_{name} of {a} and {b} gave {actual} rather than a limit"
                ),
            }
        }
    }

    /// Addition and subtraction undo each other exactly. There is no rounding
    /// in either, so this is equality rather than an approximation.
    #[test]
    fn test_addition_and_subtraction_invert(a in finite(), b in finite()) {
        if let Some(sum) = a.checked_add(b) {
            prop_assert_eq!(sum.checked_sub(b), Some(a), "{} + {} - {}", a, b, b);
            prop_assert_eq!(sum, b + a, "addition is not commutative for {} and {}", a, b);
        }
    }

    /// The midpoint sits between its ends and cannot overflow, whatever they
    /// are: the sum is formed in a wider space than either operand.
    #[test]
    fn test_the_midpoint_lies_between_its_ends(a in finite(), b in finite()) {
        let mid = a.midpoint(b);
        prop_assert!(mid.is_finite(), "midpoint of {a} and {b} overflowed");
        prop_assert!(mid >= a.min(b) && mid <= a.max(b), "{mid} outside [{a}, {b}]");
    }

    /// Negation and `abs` are exact for every value, which is what holding
    /// `i128::MIN` out of the finite range buys: the range is symmetric, so
    /// every value has an opposite and every value has a magnitude.
    #[test]
    fn test_negation_and_abs_are_total(value in finite()) {
        let negated = -value;
        prop_assert!(negated.is_finite(), "negating {value} left the range");
        prop_assert_eq!(-negated, value, "negating {} twice", value);
        prop_assert!(value.abs().is_finite(), "abs of {value} left the range");
        prop_assert!(!value.abs().is_sign_negative(), "abs of {value} stayed negative");
        prop_assert_eq!(value.abs(), if value.is_sign_negative() { negated } else { value });
    }

    /// The ordering is total and agrees with the underlying integer, NaN
    /// included: it sorts below every finite value rather than refusing to
    /// compare, which is what keeps `Dec` usable as a key and in a sorted book.
    #[test]
    fn test_the_ordering_is_total_and_agrees_with_the_raw(values in proptest::collection::vec(any_dec(), 0..32)) {
        let mut sorted = values.clone();
        sorted.sort();
        let mut raws: Vec<i128> = values.iter().map(|value| value.into_raw()).collect();
        raws.sort_unstable();
        let sorted_raws: Vec<i128> = sorted.iter().map(|value| value.into_raw()).collect();
        prop_assert_eq!(sorted_raws, raws);
        // and NaN is the least element wherever it appears
        if values.contains(&Dec::NAN) {
            prop_assert_eq!(sorted.first(), Some(&Dec::NAN));
        }
    }
}
