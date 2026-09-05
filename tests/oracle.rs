//! Multiplication, division and the square root checked against an exact
//! 256-bit reference.
//!
//! `mul_raw` computes `a * b / 10^18`, `div_raw` computes `a * 10^18 / b`, and
//! the root is taken over `a * 10^18`. No such intermediate fits a `u128` -
//! 256 bits for the first, 188 for the second, 187 for the third - and
//! reaching the answer in steps that do is the whole reason those modules are
//! shaped the way they are: the modular inverse, the `10^9` chunking, the
//! three-way split between the fast, chunked and bit-walk paths, and the
//! comparison that settles a squaring without ever performing one. Their own
//! tests check those paths against each other, which cannot
//! catch a mistake shared by all of them, because every path funnels through
//! the same rounding helper and the same `divide first, scale the remainder`
//! premise.
//!
//! The reference here makes the overflow disappear instead of working around
//! it: a wide integer, schoolbook multiplication, and long division a bit at a
//! time. It is thousands of times slower and shares no constant, no path and
//! no helper with the code it checks, so agreement between the two is evidence
//! rather than a tautology.

use std::cmp::Ordering;

use troy::Dec;

const ONE: u128 = 1_000_000_000_000_000_000;
const MAX_RAW: u128 = i128::MAX as u128;

// -- the reference arithmetic ------------------------------------------------

/// A 256-bit unsigned integer as four little-endian limbs. Deliberately naive:
/// it is trusted because it can be read, not because it is itself tested.
#[derive(Clone, Copy, PartialEq, Eq)]
struct U256([u64; 4]);

impl U256 {
    const ZERO: Self = Self([0; 4]);

    fn from_u128(value: u128) -> Self {
        Self([value as u64, (value >> 64) as u64, 0, 0])
    }

    /// `None` once the value needs more than 128 bits, which is how a product
    /// past the finite range is recognised.
    fn to_u128(self) -> Option<u128> {
        match self.0[2] | self.0[3] {
            0 => Some(self.0[0] as u128 | (self.0[1] as u128) << 64),
            _ => None,
        }
    }

    fn bit(self, index: u32) -> bool {
        self.0[index as usize / 64] >> (index % 64) & 1 == 1
    }

    /// Index of the highest set bit, `None` for zero.
    fn highest_bit(self) -> Option<u32> {
        (0..4)
            .rev()
            .find(|&limb| self.0[limb] != 0)
            .map(|limb| limb as u32 * 64 + 63 - self.0[limb].leading_zeros())
    }

    fn shl1(self) -> Self {
        let mut limbs = [0_u64; 4];
        let mut carry = 0;
        for (index, limb) in self.0.into_iter().enumerate() {
            limbs[index] = limb << 1 | carry;
            carry = limb >> 63;
        }
        Self(limbs)
    }

    fn set_low_bit(self) -> Self {
        let mut limbs = self.0;
        limbs[0] |= 1;
        Self(limbs)
    }

    /// Wrapping, and only ever called where the result is known non-negative.
    fn sub(self, other: Self) -> Self {
        let mut limbs = [0_u64; 4];
        let mut borrow = 0;
        for ((slot, left), right) in limbs.iter_mut().zip(self.0).zip(other.0) {
            let (difference, first) = left.overflowing_sub(right);
            let (difference, second) = difference.overflowing_sub(borrow);
            *slot = difference;
            // both cannot borrow at once: the first only wraps to a non-zero
            // difference, which the second then has room to take one from
            borrow = (first | second) as u64;
        }
        Self(limbs)
    }

    fn add1(self) -> Self {
        let mut limbs = self.0;
        for limb in limbs.iter_mut() {
            let (value, carry) = limb.overflowing_add(1);
            *limb = value;
            if !carry {
                break;
            }
        }
        Self(limbs)
    }

    /// The integer square root, two bits of the radicand at a time: the same
    /// restoring shape as `divmod`, and correct for the same visible reason.
    /// It shares nothing with the crate's root but the answer.
    fn isqrt(self) -> Self {
        let Some(top) = self.highest_bit() else {
            return Self::ZERO;
        };
        let mut root = Self::ZERO;
        let mut remainder = Self::ZERO;
        for pair in (0..=top / 2).rev() {
            // bring down the next two bits
            remainder = remainder.shl1();
            if self.bit(pair * 2 + 1) {
                remainder = remainder.set_low_bit();
            }
            remainder = remainder.shl1();
            if self.bit(pair * 2) {
                remainder = remainder.set_low_bit();
            }
            // the next digit of the root is a one exactly when what it would
            // subtract, `2 * root + 1`, still fits in the remainder
            root = root.shl1();
            let candidate = root.shl1().set_low_bit();
            if remainder >= candidate {
                remainder = remainder.sub(candidate);
                root = root.set_low_bit();
            }
        }
        root
    }
}

impl Ord for U256 {
    fn cmp(&self, other: &Self) -> Ordering {
        // most significant limb first, which the derive would get backwards
        self.0.iter().rev().cmp(other.0.iter().rev())
    }
}

impl PartialOrd for U256 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// The full 256-bit product, as four 64x64 partials.
fn mul_u128(a: u128, b: u128) -> U256 {
    let (a_low, a_high) = (a as u64 as u128, a >> 64);
    let (b_low, b_high) = (b as u64 as u128, b >> 64);
    let low = a_low * b_low;
    let (first, second) = (a_low * b_high, a_high * b_low);
    let high = a_high * b_high;

    let middle = (low >> 64) + (first & u64::MAX as u128) + (second & u64::MAX as u128);
    let upper = (middle >> 64) + (first >> 64) + (second >> 64) + (high & u64::MAX as u128);
    U256([
        low as u64,
        middle as u64,
        upper as u64,
        ((upper >> 64) + (high >> 64)) as u64,
    ])
}

/// Long division a bit at a time: the slowest correct way to do it, and the
/// one whose correctness is visible on the page.
fn divmod(numerator: U256, divisor: U256) -> (U256, U256) {
    let mut quotient = U256::ZERO;
    let mut remainder = U256::ZERO;
    // bits above the numerator's highest contribute nothing to either side, so
    // starting below them is a shortcut rather than a difference
    let Some(top) = numerator.highest_bit() else {
        return (U256::ZERO, U256::ZERO);
    };
    for bit in (0..=top).rev() {
        remainder = remainder.shl1();
        if numerator.bit(bit) {
            remainder = remainder.set_low_bit();
        }
        quotient = quotient.shl1();
        if remainder >= divisor {
            remainder = remainder.sub(divisor);
            quotient = quotient.set_low_bit();
        }
    }
    (quotient, remainder)
}

/// `numerator / divisor` with a tie going away from zero, which on magnitudes
/// is simply up.
fn round_div(numerator: U256, divisor: U256) -> U256 {
    let (quotient, remainder) = divmod(numerator, divisor);
    match remainder.shl1() >= divisor {
        true => quotient.add1(),
        false => quotient,
    }
}

/// Apply the sign and reject anything the finite range cannot hold. The range
/// is symmetric, so one limit serves both signs.
fn finish(negative: bool, magnitude: U256) -> Option<i128> {
    let magnitude = magnitude.to_u128().filter(|value| *value <= MAX_RAW)?;
    Some(match negative {
        true => -(magnitude as i128),
        false => magnitude as i128,
    })
}

/// What `Dec::checked_mul` should return, in raws.
fn oracle_mul(a: i128, b: i128) -> Option<i128> {
    // i128::MIN is the NaN pattern, and NaN poisons the result
    if a == i128::MIN || b == i128::MIN {
        return None;
    }
    let product = mul_u128(a.unsigned_abs(), b.unsigned_abs());
    finish((a < 0) != (b < 0), round_div(product, U256::from_u128(ONE)))
}

/// What `Dec::checked_div` should return, in raws.
fn oracle_div(a: i128, b: i128) -> Option<i128> {
    if a == i128::MIN || b == i128::MIN || b == 0 {
        return None;
    }
    let scaled = mul_u128(a.unsigned_abs(), ONE);
    finish(
        (a < 0) != (b < 0),
        round_div(scaled, U256::from_u128(b.unsigned_abs())),
    )
}

/// What `Dec::checked_sqrt` should return, in raws.
fn oracle_sqrt(a: i128) -> Option<i128> {
    // i128::MIN is the NaN pattern, and every negative value answers the same
    // way: there is no root here to return
    if a < 0 {
        return None;
    }
    let scaled = mul_u128(a.unsigned_abs(), ONE);
    let root = scaled.isqrt().to_u128()?;
    // half away from zero, which on a magnitude is up. The remainder passing
    // the root is the same test as `a * 10^18 >= root^2 + root + 1`, and no
    // input can land on the tie itself.
    let rounded = match scaled.sub(mul_u128(root, root)) > U256::from_u128(root) {
        true => root + 1,
        false => root,
    };
    finish(false, U256::from_u128(rounded))
}

// -- inputs ------------------------------------------------------------------

/// The same xorshift the unit tests use, so a failure is reproducible.
struct Xorshift(u64);

impl Xorshift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// A raw spread across the whole finite range. A uniform draw would be
    /// enormous every time and never exercise a small value, so the width is
    /// drawn first and the digits after it; a quarter of the draws are
    /// quantised to nine decimal places, which is the granularity real prices
    /// and sizes carry and the one both fast paths are built for.
    fn raw(&mut self) -> i128 {
        let width = (self.next() % 128) as u32;
        let digits = (self.next() as u128) << 64 | self.next() as u128;
        let magnitude = digits >> (127 - width);
        let magnitude = match self.next() % 4 {
            0 => magnitude / 1_000_000_000 * 1_000_000_000,
            _ => magnitude,
        };
        let magnitude = (magnitude % (MAX_RAW + 1)) as i128;
        match self.next() % 2 {
            0 => -magnitude,
            _ => magnitude,
        }
    }
}

/// Raws worth trying in every combination: the small integers, the
/// neighbourhood of every power of ten, the halves that decide a tie, the ends
/// of the range and the NaN pattern.
fn boundary_raws() -> Vec<i128> {
    let mut raws = vec![i128::MIN, i128::MAX, -i128::MAX, i128::MAX / 2];
    for value in 0..SMALL_INTEGERS {
        raws.push(value);
        raws.push(-value);
    }
    for exponent in exponents() {
        let power = 10_i128.pow(exponent);
        for offset in [-1, 0, 1] {
            raws.push(power + offset);
            raws.push(-(power + offset));
            raws.push(power / 2 + offset);
        }
    }
    raws.sort_unstable();
    raws.dedup();
    raws
}

/// Powers of ten to build the sweep around. The sweep is quadratic in the
/// number of raws and the reference is a few hundred branches per division, so
/// an unoptimised build takes a slice: every third exponent, plus the ones that
/// bound something - a whole unit at 0, the `10^9` chunking, the scale at 18,
/// the top of the range. A release build is ninety times faster and takes the
/// lot.
fn exponents() -> Vec<u32> {
    match cfg!(debug_assertions) {
        true => (0..=38)
            .filter(|exponent| exponent % 3 == 0 || matches!(exponent, 9 | 18 | 19 | 38))
            .collect(),
        false => (0..=38).collect(),
    }
}

const SMALL_INTEGERS: i128 = match cfg!(debug_assertions) {
    true => 16,
    false => 64,
};

const RANDOM_CASES: usize = match cfg!(debug_assertions) {
    true => 20_000,
    false => 300_000,
};

// -- the tests ---------------------------------------------------------------

#[test]
fn test_multiplication_matches_the_reference() {
    let mut rng = Xorshift(0x1234_5678_9abc_def1);
    for _ in 0..RANDOM_CASES {
        let (a, b) = (rng.raw(), rng.raw());
        let product = Dec::from_raw(a).checked_mul(Dec::from_raw(b));
        assert_eq!(product.map(Dec::into_raw), oracle_mul(a, b), "{a} * {b}");
    }
}

#[test]
fn test_division_matches_the_reference() {
    let mut rng = Xorshift(0xfeed_face_dead_beef);
    for _ in 0..RANDOM_CASES {
        let (a, b) = (rng.raw(), rng.raw());
        let quotient = Dec::from_raw(a).checked_div(Dec::from_raw(b));
        assert_eq!(quotient.map(Dec::into_raw), oracle_div(a, b), "{a} / {b}");
    }
}

#[test]
fn test_every_pair_of_boundary_raws_matches_the_reference() {
    // ties, powers of ten and the ends of the range are where a rounding rule
    // or a range check goes wrong, and they are far too sparse to be drawn
    let raws = boundary_raws();
    for &a in &raws {
        let root = Dec::from_raw(a).checked_sqrt();
        assert_eq!(root.map(Dec::into_raw), oracle_sqrt(a), "sqrt {a}");
        for &b in &raws {
            let product = Dec::from_raw(a).checked_mul(Dec::from_raw(b));
            assert_eq!(product.map(Dec::into_raw), oracle_mul(a, b), "{a} * {b}");
            let quotient = Dec::from_raw(a).checked_div(Dec::from_raw(b));
            assert_eq!(quotient.map(Dec::into_raw), oracle_div(a, b), "{a} / {b}");
        }
    }
}

#[test]
fn test_the_random_raws_reach_every_division_path() {
    // agreement is only worth as much as the ground it covers, so the same
    // draw that feeds the tests above is asked which path it took. The
    // divisions below mirror the ones in `div_raw`.
    const CHUNK: u128 = 1_000_000_000;
    const CHUNK_LIMIT: u128 = u128::MAX / CHUNK;

    let mut rng = Xorshift(0xfeed_face_dead_beef);
    let (mut cancelled, mut chunked, mut bit_walk) = (0, 0, 0);
    for _ in 0..RANDOM_CASES {
        let (_, b) = (rng.raw(), rng.raw());
        match b.unsigned_abs() {
            0 => continue,
            divisor if divisor.is_multiple_of(CHUNK) => cancelled += 1,
            divisor if divisor > CHUNK_LIMIT => bit_walk += 1,
            _ => chunked += 1,
        }
    }
    let floor = RANDOM_CASES / 100;
    assert!(cancelled > floor, "only {cancelled} cancelled the scaling");
    assert!(chunked > floor, "only {chunked} took the 10^9 steps");
    assert!(bit_walk > floor, "only {bit_walk} took the bit walk");
}

#[test]
fn test_the_square_root_matches_the_reference() {
    let mut rng = Xorshift(0x0bad_c0de_1234_5678);
    for _ in 0..RANDOM_CASES {
        let a = rng.raw();
        let root = Dec::from_raw(a).checked_sqrt();
        assert_eq!(root.map(Dec::into_raw), oracle_sqrt(a), "sqrt {a}");
    }
}

#[test]
fn test_the_random_raws_reach_both_square_root_paths() {
    // the root splits where `raw * 10^18` stops fitting a u128, at a value of
    // about 340, and agreement above the split says nothing about below it
    const SIMPLE_MAX: u128 = u128::MAX / ONE;

    let mut rng = Xorshift(0x0bad_c0de_1234_5678);
    let (mut simple, mut wide) = (0, 0);
    for _ in 0..RANDOM_CASES {
        match rng.raw() {
            negative if negative < 0 => continue,
            small if small as u128 <= SIMPLE_MAX => simple += 1,
            _ => wide += 1,
        }
    }
    let floor = RANDOM_CASES / 100;
    assert!(simple > floor, "only {simple} took the simple path");
    assert!(wide > floor, "only {wide} took the wide path");
}
