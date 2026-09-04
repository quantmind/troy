This page records the decisions behind [`Dec`](crate::Dec) and the reasoning
that led to them. It is the place to look before changing how a value is
represented, how an operation reports failure, or how two values compare.

## Goal

Troy provides numeric primitives for high frequency trading, where a decimal
operation sits on the critical path between a market data message and an order.
Correctness and latency are the two priorities, and where they conflict the
design prefers a representation that makes the fast case branch-free over one
that makes the failing case convenient.

`Dec` is deliberately not a general purpose decimal. It carries a fixed scale,
so no operation ever renegotiates it, and no value carries a scale field that
has to be read, compared, or aligned.

## Representation

A `Dec` is a single `i128` holding the value scaled by `10^SCALE`, where
[`Dec::SCALE`](crate::Dec::SCALE) is 18. The type is `Copy`, 16 bytes, and has
no niche beyond the one described below. The
[memory layout](crate::memory_layout) page compares those 16 bytes with the
other decimals the benchmarks measure.

```rust
use troy::{Dec, dec};

assert_eq!(dec!(1.5).into_raw(), 1_500_000_000_000_000_000);
assert_eq!(Dec::ONE.into_raw(), 1_000_000_000_000_000_000);
```

Eighteen places is enough for every venue's price and size increment with room
left for intermediate products, and it leaves a finite range of roughly
±1.7e20 — seven orders of magnitude above any notional that will ever be
priced.

## The finite range is symmetric

`i128` is asymmetric: `i128::MIN` has no positive counterpart, so `-x` and
`x.abs()` are partial functions on it. Rather than special case `2^127` in
every magnitude check, `Dec` holds `i128::MIN` out of the finite range:

| constant | raw |
| --- | --- |
| [`Dec::MAX`](crate::Dec::MAX) | `i128::MAX` |
| [`Dec::MIN`](crate::Dec::MIN) | `-i128::MAX` |
| [`Dec::NAN`](crate::Dec::NAN) | `i128::MIN` |

Reserving one value in `2^128` buys a range where negation and absolute value
are total and exact, and it leaves exactly one spare bit pattern.

```rust
use troy::Dec;

assert_eq!(-Dec::MIN, Dec::MAX);
assert_eq!(Dec::MIN.into_raw(), -Dec::MAX.into_raw());
```

## The spare pattern is NaN

That spare pattern is [`Dec::NAN`](crate::Dec::NAN), the not-a-number state.
Every operation that leaves the finite range returns it, and every operation
handed it returns it, so a fault propagates to wherever the result is finally
examined instead of being resolved at the point it occurs.

The alternatives were considered and rejected:

- **Saturating** answers an overflow with a plausible looking figure. A price
  clamped to `Dec::MAX` passes every downstream sanity check and reaches the
  wire as an order.
- **Panicking** puts an abort in a matching engine's hot path, which is a worse
  failure mode than a wrong number in most trading systems.
- **`Option` everywhere** costs an unwrap at every step of an expression and
  makes the common path read like the failing one.

```rust
use troy::{Dec, dec};

// overflow poisons the result rather than clamping it
assert!((Dec::MAX + Dec::ONE).is_nan());
assert!((Dec::MAX * dec!(2)).is_nan());

// and the poison survives everything downstream
assert!((Dec::NAN * dec!(0)).is_nan());
assert!((Dec::NAN - Dec::NAN).is_nan());
assert!(Dec::NAN.abs().is_nan());
assert!(Dec::NAN.round_dp(2).is_nan());
```

Callers who want the fault resolved on the spot still have the explicit forms:
`checked_*` returns `None`, `saturating_*` clamps to the finite bounds. Both
treat a NaN operand as a failure.

```rust
use troy::Dec;

assert_eq!(Dec::MAX.checked_add(Dec::ONE), None);
assert_eq!(Dec::MAX.saturating_add(Dec::ONE), Dec::MAX);
assert_eq!(Dec::NAN.checked_add(Dec::ONE), None);
assert!(Dec::NAN.saturating_add(Dec::ONE).is_nan());
```

## Comparison keeps a total order

This is the one place `Dec` deliberately departs from IEEE 754. `Dec::NAN`
**equals itself** and orders **below** [`Dec::MIN`](crate::Dec::MIN):

```rust
use troy::Dec;

assert_eq!(Dec::NAN, Dec::NAN);
assert!(Dec::NAN < Dec::MIN);
```

An IEEE NaN is unordered, which would force `PartialEq` to be irreflexive and
so remove `Eq`, and `Ord` with it. For a decimal whose primary container is a
sorted book that is too high a price: `BTreeMap<Dec, Level>`, `HashMap` keys,
`sort`, `binary_search`, `min`/`max` and `#[derive(Eq)]` on any struct holding
a `Dec` all depend on those traits. Comparison is also the cheapest operation
in the crate at roughly one `i128` compare, and IEEE semantics would double it
for no benefit on the path that matters.

The justification is that a `Dec` NaN is not an IEEE NaN. It never arises from
a legitimate calculation like `0.0 / 0.0`; it means an overflow happened, which
is a bug in the inputs or in an accumulation. Treating it as an ordered
sentinel is both cheaper and easier to reason about.

The consequence to be aware of: **a NaN wins any `min` reduction and sorts to
the front.** Ordering it below `Dec::MIN` rather than above `Dec::MAX` is
chosen so that a poisoned value is conspicuous at the head of a sorted book
rather than buried at the tail.

```rust
use troy::Dec;

let values = [Dec::ZERO, Dec::NAN, Dec::MIN];
assert_eq!(values.iter().min(), Some(&Dec::NAN));
```

## Check at the boundary

Because arithmetic propagates rather than reports, the check belongs where a
value leaves the system — an order about to be sent, a figure about to be
persisted or published — not at every intermediate step.
[`is_finite`](crate::Dec::is_finite) is that check.

```rust
use troy::{Dec, dec};

fn submit(price: Dec, size: Dec) -> Option<Dec> {
    let notional = price * size;
    notional.is_finite().then_some(notional)
}

assert_eq!(submit(dec!(104_237.25), dec!(0.5)), Some(dec!(52_118.625)));
assert_eq!(submit(Dec::MAX, Dec::MAX), None);
```

A NaN is neither positive, negative, nor zero, so the sign predicates cannot be
used as a finiteness test:

```rust
use troy::Dec;

assert!(!Dec::NAN.is_zero());
assert!(!Dec::NAN.is_sign_negative());
assert!(!Dec::NAN.is_sign_positive());
```

## Cost: fold the check into an existing one

The NaN state was measured, not assumed, and the result shaped the
implementation. Testing `is_nan()` at the top of every operation cost 15% on
multiplication, 43% on `ceil` and 51% on `to_f64`. Nearly all of that was
recovered by observing that **most operations already have a check that a NaN
fails**:

- `floor` needs no test. Flooring `2^127` rounds away from zero and the
  reconstruction then overflows, which is already the NaN answer.
- `ceil` is written as `-floor(-x)`. Wrapping negation maps NaN to itself and
  every finite raw to its exact opposite, so it inherits floor's handling.
- `abs` and `neg` use `wrapping_abs`/`wrapping_neg`, which fix `i128::MIN` to
  itself. Propagation is free and branchless.
- multiplication tests inside `mul_wide` rather than on the fast path. A NaN
  has magnitude `2^127`, which `10^9` does not divide, so it can never take the
  exact-division fast path.
- `to_f64` tests after the fast path, for the same reason.

Where no existing check catches it, the test is real and stays: `trunc`,
`round_dp`, and `round_to_step` all need one. The last is load-bearing rather
than defensive — with a step of `0.001` the fraction rounds *down*, so a NaN
would otherwise reconstruct into the finite range and read as
`-170141183460469231731.687`.

The general rule for new operations: **look for an overflow or exactness check
that a `2^127` magnitude already fails, and let it do the work.**

## Division reaches 187 bits in u128 steps

The raw quotient is `a * 10^18 / b`, a product needing 187 bits that no `u128`
holds. Rather than carry a 256-bit intermediate, every path divides first and
scales the remainder afterwards, which keeps each step inside a `u128`:

- **The divisor carries at most 9 decimals**, so `10^9` divides its raw
  exactly. Cancelling that against the `10^18` leaves `a * 10^9 / b9`, done as
  a quotient and a scaled remainder — two divisions and no wide arithmetic.
  Every price, size and tick takes this path.
- **Finer than that**, `a` is reduced below `b` first, then the remaining
  `10^18` is applied to the remainder in two `10^9` steps. The remainder is
  below `b`, so each step has room. This needs `|b|` at or below `3.4e11`.
- **A divisor above `3.4e11` with more than 9 decimals** has room for neither
  step, so the scaling walks the 60 bits of `10^18` one at a time, keeping a
  window below `b`. Doubling that window or adding the remainder to it stays
  under twice `b`, so the 187-bit product is never formed. It is slow and
  effectively unreachable from market data, but it keeps the operation exact
  over the whole range rather than returning NaN for inputs that have an
  answer.

The three paths overlap on small divisors, where the tests run them against
each other: a wrong result would have to be produced identically by three
independent routines.

Dividing by zero yields NaN. There is no infinity in the type, so it is the
same class of fault as an overflow and collapses to the same state.

## Prefer a branch to a select

Overflow handling should be written so the compiler emits a predicted branch
rather than a conditional move. A never-taken branch is speculated through and
never joins the loop-carried dependency chain; a `cmov` sits on it and adds a
cycle to every iteration.

This is worth more than it sounds. Replacing `saturating_add` — whose
sign-dependent select was on the dependency chain — with a checked add that
branches to NaN made addition **57% faster**, from 2.03 ns to 0.87 ns per
operation, moving `Dec` past `rust_decimal` on that benchmark.

## Open questions

- `Display` renders NaN as `"NaN"` but `FromStr` does not accept it, so a NaN
  does not round trip through text, and `serde` will serialise a value it
  cannot read back.
- `checked_*` returns `None` both for an overflowing result and for a NaN
  operand, conflating "too big" with "already invalid".
