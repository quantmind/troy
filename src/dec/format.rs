use super::core::{Dec, ONE_U};
use std::fmt;

// sign, the 21 integer digits of i128::MIN / 10^18, the point, and the fraction
const BUFFER_LEN: usize = 1 + 21 + 1 + Dec::SCALE as usize;

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

fn render(buffer: &mut [u8; BUFFER_LEN], raw: i128) -> usize {
    let magnitude = raw.unsigned_abs();
    let integer = magnitude / ONE_U;
    let mut fraction = (magnitude % ONE_U) as u64;

    let mut position = 0;
    if raw < 0 {
        buffer[0] = b'-';
        position = 1;
    }

    position += match u64::try_from(integer) {
        Ok(value) => {
            let count = digits(value);
            write_digits(&mut buffer[position..], value, count);
            count
        }
        // only a 20- or 21-digit integer reaches here, and peeling one digit
        // brings the rest inside a u64, where division by ten is a multiply
        Err(_) => {
            let last = (integer % 10) as u8;
            let head = (integer / 10) as u64;
            let count = digits(head);
            write_digits(&mut buffer[position..], head, count);
            buffer[position + count] = b'0' + last;
            count + 1
        }
    };

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

impl fmt::Display for Dec {
    #[inline(always)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut buffer = [0_u8; BUFFER_LEN];
        let length = render(&mut buffer, self.0);
        f.write_str(std::str::from_utf8(&buffer[..length]).unwrap_or(""))
    }
}

impl fmt::Debug for Dec {
    #[inline(always)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
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
