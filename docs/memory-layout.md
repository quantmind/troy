# Memory Layout

`Dec` is a fixed-scale decimal: the scale is a compile-time constant
([`Dec::SCALE`]), not a field. That single decision is what lets it carry a
full 128-bit coefficient in the same 16 bytes a 96-bit decimal needs, which
matters when a book of quotes has to stay in cache.

The types below are the ones the [benchmarks](https://github.com/quantmind/troy)
compare against. One character is one byte:

```text
                     0        8       16       24
                     │        │        │        │
  troy::Dec          [MMMMMMMMMMMMMMMM]              16 B
  rust_decimal       [FFFFMMMMMMMMMMMM]              16 B
  fastnum::D128      [MMMMMMMMMMMMMMMMCCCCCCCC]      24 B
  f64                [VVVVVVVV]                       8 B

  M = mantissa/coefficient   F = flags   C = control block
```

| type | coefficient | scale | size |
| ---- | ----------- | ----- | ---- |
| `troy::Dec` | 128-bit signed | compile-time constant | 16 B |
| `rust_decimal::Decimal` | 96-bit | runtime, packed into spare `flags` bits | 16 B |
| `fastnum::D128` | 128-bit unsigned | runtime, in a separate control block | 24 B |
| `f64` | 53-bit (binary, inexact) | binary exponent | 8 B |

```
assert_eq!(size_of::<troy::Dec>(), 16);
```

## Where the bytes go

`Dec` is a newtype over `i128`. Every bit is significant digits and sign; the
18 decimal places are implied by [`Dec::SCALE`] and occupy no space at all.

```text
  troy::Dec  =  i128
  ┌────────────────────────────────────────────┐
  │ sign + 127 mantissa bits                   │   scale is NOT here:
  └────────────────────────────────────────────┘   const SCALE = 18
   0                                        127
```

`rust_decimal` uses the .NET `System.Decimal` layout: four `u32` words, three
of which form a 96-bit integer. Because the coefficient stops at 96 bits,
there is a whole spare word for the scale and sign — 23 bits of it unused.

```text
  rust_decimal::Decimal  =  4 × u32
  ┌─────────┬─────────┬─────────┬─────────┐
  │  flags  │   hi    │   lo    │   mid   │
  └─────────┴─────────┴─────────┴─────────┘
       │      └────────┬────────┘
       │        96-bit mantissa
       │
       └── bits 0-15  unused
           bits 16-23 scale (0-28)
           bits 24-30 unused
           bit  31    sign
```

`fastnum` cannot play that trick. A full 128-bit coefficient already fills
16 bytes, so the scale, sign, accumulated signals and rounding mode need a
further 8-byte control block.

```text
  fastnum::Decimal<2>  =  UInt<2> + ControlBlock
  ┌───────────────────────────┬──────────────┐
  │ digits: 2 × u64 = 128 bit │ cb: u64      │
  └───────────────────────────┴──────────────┘
                                └── scale (16 bits) + sign
                                    + signals + rounding mode
```

## The trade-off

Against `rust_decimal`, `Dec` buys 32 extra coefficient bits — 38 significant
digits instead of about 28 — for free. Against `fastnum::D128` it holds the
same 128-bit coefficient in two thirds of the space.

What it gives up is dynamic range. A `rust_decimal` at scale 0 reaches roughly
7.9 × 10^28, because it can spend all 96 bits on the integer part. A `Dec`
always spends 18 digits on the fraction, so [`Dec::MAX`] is `i128::MAX` scaled
down: a little over 1.7 × 10^20. For prices, sizes and notionals that is ample;
for astronomy it is not.

[`Dec::SCALE`]: crate::Dec::SCALE
[`Dec::MAX`]: crate::Dec::MAX
