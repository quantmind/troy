use super::core::{Dec, ONE_U, POW10};
use std::fmt::{self, Write};

// sign, the 21 integer digits of i128::MIN / 10^18, the point, and the fraction
const BUFFER_LEN: usize = 1 + 21 + 1 + Dec::SCALE as usize;

// a precision past Dec::SCALE is met with trailing zeros, written in blocks
// rather than one call per zero
const ZEROS: &str = "00000000000000000000000000000000";

const POW10_64: [u64; 20] = {
    let mut table = [1_u64; 20];
    let mut index = 1;
    while index < 20 {
        table[index] = table[index - 1] * 10;
        index += 1;
    }
    table
};

fn digits(value: u64) -> usize {
    let mut count = 1;
    while count < POW10_64.len() && value >= POW10_64[count] {
        count += 1;
    }
    count
}

fn write_digits(out: &mut [u8], mut value: u64, count: usize) {
    let mut index = count;
    while index > 0 {
        index -= 1;
        out[index] = b'0' + (value % 10) as u8;
        value /= 10;
    }
}

fn write_integer(buffer: &mut [u8; BUFFER_LEN], position: usize, integer: u128) -> usize {
    match u64::try_from(integer) {
        Ok(value) => {
            let count = digits(value);
            write_digits(&mut buffer[position..], value, count);
            position + count
        }
        // only a 20- or 21-digit integer reaches here, and peeling one digit
        // brings the rest inside a u64, where division by ten is a multiply
        Err(_) => {
            let last = (integer % 10) as u8;
            let head = (integer / 10) as u64;
            let count = digits(head);
            write_digits(&mut buffer[position..], head, count);
            buffer[position + count] = b'0' + last;
            position + count + 1
        }
    }
}

// every place the value carries, trailing zeros stripped
fn render_natural(buffer: &mut [u8; BUFFER_LEN], position: usize, magnitude: u128) -> usize {
    let mut position = write_integer(buffer, position, magnitude / ONE_U);
    let mut fraction = (magnitude % ONE_U) as u64;

    if fraction == 0 {
        return position;
    }

    // strip trailing zeros in five steps rather than one division per zero
    let mut length = Dec::SCALE as usize;
    let mut step = 16;
    while step > 0 {
        let factor = POW10_64[step];
        if fraction.is_multiple_of(factor) {
            fraction /= factor;
            length -= step;
        }
        step /= 2;
    }

    buffer[position] = b'.';
    position += 1;
    write_digits(&mut buffer[position..], fraction, length);
    position + length
}

// exactly `places` decimal places, halves away from zero. The rounding happens
// on the magnitude rather than through round_dp, so a carry that leaves the
// finite range - Dec::MAX at zero places - renders as the number it rounded to
// instead of as NaN
fn render_fixed(
    buffer: &mut [u8; BUFFER_LEN],
    position: usize,
    magnitude: u128,
    places: usize,
) -> usize {
    let factor = POW10[Dec::SCALE as usize - places] as u128;
    // the magnitude is at most i128::MAX and half the factor at most 5e17, so
    // the carry cannot leave a u128
    let scaled = (magnitude + factor / 2) / factor;
    let unit = POW10[places] as u128;

    let mut position = write_integer(buffer, position, scaled / unit);
    if places == 0 {
        return position;
    }

    buffer[position] = b'.';
    position += 1;
    write_digits(&mut buffer[position..], (scaled % unit) as u64, places);
    position + places
}

#[inline(always)]
fn render(buffer: &mut [u8; BUFFER_LEN], raw: i128, places: Option<usize>, plus: bool) -> usize {
    let mut position = 0;
    if raw < 0 {
        buffer[0] = b'-';
        position = 1;
    } else if plus {
        buffer[0] = b'+';
        position = 1;
    }

    let magnitude = raw.unsigned_abs();
    match places {
        Some(places) => render_fixed(buffer, position, magnitude, places),
        None => render_natural(buffer, position, magnitude),
    }
}

fn write_zeros(f: &mut fmt::Formatter<'_>, mut count: usize) -> fmt::Result {
    while count > 0 {
        let step = count.min(ZEROS.len());
        f.write_str(&ZEROS[..step])?;
        count -= step;
    }
    Ok(())
}

fn write_fill(f: &mut fmt::Formatter<'_>, fill: char, count: usize) -> fmt::Result {
    for _ in 0..count {
        f.write_char(fill)?;
    }
    Ok(())
}

fn write_body(f: &mut fmt::Formatter<'_>, text: &str, zeros: usize) -> fmt::Result {
    f.write_str(text)?;
    write_zeros(f, zeros)
}

// Formatter::pad reads a precision as a length to truncate to, which would cut
// "34.50" back to "34", so the width is applied here instead
fn pad(f: &mut fmt::Formatter<'_>, text: &str, zeros: usize) -> fmt::Result {
    let Some(width) = f.width() else {
        return write_body(f, text, zeros);
    };
    // every byte written is ASCII, so the length is the width it occupies
    let length = text.len() + zeros;
    if length >= width {
        return write_body(f, text, zeros);
    }
    let padding = width - length;

    if f.sign_aware_zero_pad() {
        let (sign, rest) = match text.as_bytes().first() {
            Some(b'-' | b'+') => text.split_at(1),
            _ => ("", text),
        };
        f.write_str(sign)?;
        write_zeros(f, padding)?;
        return write_body(f, rest, zeros);
    }

    let fill = f.fill();
    match f.align() {
        Some(fmt::Alignment::Left) => {
            write_body(f, text, zeros)?;
            write_fill(f, fill, padding)
        }
        Some(fmt::Alignment::Center) => {
            let left = padding / 2;
            write_fill(f, fill, left)?;
            write_body(f, text, zeros)?;
            write_fill(f, fill, padding - left)
        }
        // a number right aligns by default, as the built-in numeric types do
        Some(fmt::Alignment::Right) | None => {
            write_fill(f, fill, padding)?;
            write_body(f, text, zeros)
        }
    }
}

impl fmt::Display for Dec {
    /// Renders the value, honouring the precision, width, fill, alignment and
    /// `+` of the format string. Without a precision every place the value
    /// carries is written and trailing zeros are stripped; with one the value
    /// is rounded to exactly that many places, halves away from zero, and
    /// padded with zeros past [`Dec::SCALE`].
    ///
    /// ```
    /// use troy::{Dec, dec};
    ///
    /// assert_eq!(dec!(34.5).to_string(), "34.5");
    /// assert_eq!(format!("{:.2}", dec!(34.5)), "34.50");
    /// assert_eq!(format!("{:.2}", dec!(1.005)), "1.01");
    /// assert_eq!(format!("{:>10.2}", dec!(34.5)), "     34.50");
    /// assert_eq!(format!("{:+.2}", dec!(34.5)), "+34.50");
    /// assert_eq!(Dec::NAN.to_string(), "NaN");
    /// ```
    #[inline(always)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_nan() {
            return pad(f, "NaN", 0);
        }
        let mut buffer = [0_u8; BUFFER_LEN];
        let precision = f.precision();

        // the plain `{}`, with no precision to round to, no sign to force and
        // no width to pad against. Held apart so the constants fold and it
        // compiles to what it was before the format spec was honoured
        if precision.is_none() && f.width().is_none() && !f.sign_plus() {
            let length = render(&mut buffer, self.0, None, false);
            return f.write_str(std::str::from_utf8(&buffer[..length]).unwrap_or(""));
        }

        let scale = Dec::SCALE as usize;
        // the buffer holds the scale, and a precision past it is trailing zeros
        let length = render(
            &mut buffer,
            self.0,
            precision.map(|p| p.min(scale)),
            f.sign_plus(),
        );
        let text = std::str::from_utf8(&buffer[..length]).unwrap_or("");
        pad(f, text, precision.unwrap_or(0).saturating_sub(scale))
    }
}

impl fmt::Debug for Dec {
    #[inline(always)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

#[cfg(test)]
mod rendering {
    #![allow(clippy::unwrap_used)]

    use crate::Dec;
    use crate::dec;
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
    fn test_debug_matches_display() {
        for value in [dec!(1.5), dec!(-0.25), Dec::ZERO, Dec::MIN, Dec::MAX] {
            assert_eq!(format!("{value:?}"), format!("{value}"));
        }
    }
}

#[cfg(test)]
mod formatting_bounds {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::Dec;

    #[test]
    fn test_the_widest_values_format() {
        assert_eq!(
            Dec::MIN.to_string(),
            "-170141183460469231731.687303715884105727"
        );
        assert_eq!(
            Dec::MAX.to_string(),
            "170141183460469231731.687303715884105727"
        );
    }

    #[test]
    fn test_the_buffer_holds_the_longest_rendering() {
        let longest = Dec::MIN.to_string().len();
        assert_eq!(longest, 41);
        assert_eq!(BUFFER_LEN, longest);
    }
}

#[cfg(test)]
mod format_options {
    use crate::Dec;
    use crate::dec;

    #[test]
    fn test_precision_pads_to_a_fixed_number_of_places() {
        assert_eq!(format!("{:.2}", dec!(34.5)), "34.50");
        assert_eq!(format!("{:.2}", dec!(-34.5)), "-34.50");
        assert_eq!(format!("{:.2}", dec!(34)), "34.00");
        assert_eq!(format!("{:.2}", Dec::ZERO), "0.00");
        assert_eq!(format!("{:.8}", dec!(0.5)), "0.50000000");
    }

    #[test]
    fn test_precision_rounds_halves_away_from_zero() {
        assert_eq!(format!("{:.2}", dec!(1.005)), "1.01");
        assert_eq!(format!("{:.2}", dec!(-1.005)), "-1.01");
        assert_eq!(format!("{:.2}", dec!(1.004)), "1.00");
        assert_eq!(format!("{:.2}", dec!(0.005)), "0.01");
        assert_eq!(format!("{:.0}", dec!(2.5)), "3");
        assert_eq!(format!("{:.0}", dec!(-2.5)), "-3");
        assert_eq!(format!("{:.0}", dec!(2.4)), "2");
    }

    #[test]
    fn test_rounding_carries_into_the_integer_part() {
        assert_eq!(format!("{:.2}", dec!(9.999)), "10.00");
        assert_eq!(format!("{:.2}", dec!(-9.999)), "-10.00");
        assert_eq!(format!("{:.0}", dec!(0.5)), "1");
    }

    #[test]
    fn test_a_value_rounding_below_a_place_keeps_its_sign() {
        assert_eq!(format!("{:.2}", dec!(-0.004)), "-0.00");
    }

    #[test]
    fn test_precision_past_the_scale_pads_with_zeros() {
        assert_eq!(format!("{:.18}", dec!(1.5)), "1.500000000000000000");
        assert_eq!(format!("{:.20}", dec!(1.5)), "1.50000000000000000000");
        assert_eq!(format!("{:.40}", Dec::EPSILON).len(), 42);
    }

    #[test]
    fn test_the_widest_values_take_a_precision() {
        assert_eq!(
            format!("{:.18}", Dec::MAX),
            "170141183460469231731.687303715884105727"
        );
        // the carry leaves the finite range, where round_dp would give NaN
        assert!(Dec::MAX.round_dp(0).is_nan());
        assert_eq!(format!("{:.0}", Dec::MAX), "170141183460469231732");
        assert_eq!(format!("{:.0}", Dec::MIN), "-170141183460469231732");
    }

    #[test]
    fn test_width_right_aligns_as_the_numeric_types_do() {
        assert_eq!(format!("{:10.2}", dec!(34.5)), "     34.50");
        assert_eq!(format!("{:>10.2}", dec!(34.5)), "     34.50");
        assert_eq!(format!("{:10}", dec!(34.5)), "      34.5");
        // a value wider than the width is never truncated
        assert_eq!(format!("{:3.2}", dec!(1234.5)), "1234.50");
    }

    #[test]
    fn test_width_honours_alignment_and_fill() {
        assert_eq!(format!("{:<10.2}", dec!(34.5)), "34.50     ");
        assert_eq!(format!("{:^10.2}", dec!(34.5)), "  34.50   ");
        assert_eq!(format!("{:*>8.2}", dec!(34.5)), "***34.50");
        assert_eq!(format!("{:*<8.2}", dec!(34.5)), "34.50***");
    }

    #[test]
    fn test_zero_padding_keeps_the_sign_leftmost() {
        assert_eq!(format!("{:08.2}", dec!(34.5)), "00034.50");
        assert_eq!(format!("{:08.2}", dec!(-34.5)), "-0034.50");
        assert_eq!(format!("{:08}", dec!(-34.5)), "-00034.5");
    }

    #[test]
    fn test_the_plus_flag_signs_non_negative_values() {
        assert_eq!(format!("{:+}", dec!(34.5)), "+34.5");
        assert_eq!(format!("{:+.2}", dec!(34.5)), "+34.50");
        assert_eq!(format!("{:+}", Dec::ZERO), "+0");
        assert_eq!(format!("{:+.2}", dec!(-34.5)), "-34.50");
        assert_eq!(format!("{:+09.2}", dec!(34.5)), "+00034.50");
        assert_eq!(format!("{:+.18}", Dec::MAX).len(), 41);
    }

    #[test]
    fn test_nan_takes_the_width_but_not_the_precision() {
        assert_eq!(format!("{:>5}", Dec::NAN), "  NaN");
        assert_eq!(format!("{:<5}", Dec::NAN), "NaN  ");
        assert_eq!(format!("{:.2}", Dec::NAN), "NaN");
    }

    #[test]
    fn test_debug_honours_the_same_options() {
        assert_eq!(format!("{:>8.2?}", dec!(34.5)), "   34.50");
    }
}
