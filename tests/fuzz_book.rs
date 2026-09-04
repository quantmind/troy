//! `OrderBook` under a depth cap: what a compact book must hold true however
//! the updates arrive.
//!
//! A capped side is the configuration a feed handler actually runs, and it is
//! the one with a policy in it. `set` refuses a level that would land past the
//! cap, and trims after one that lands inside it, so the side both drops
//! levels it is given and evicts levels it already held. Those two paths are
//! what these properties cover.
//!
//! The invariants are checked against the shape of the side rather than
//! against a second copy of the policy. A model that reimplemented `set` would
//! reject the same levels for the same reasons, and agree with a bug.

use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use std::collections::BTreeMap;
use troy::{Dec, OrderBook, OrderBookSide, dec};

/// The caps a compact book is run at. Ten is a top-of-book feed, two hundred
/// is past what most venues publish.
const CAPS: [usize; 4] = [10, 50, 100, 200];

const TICK: Dec = dec!(0.01);
const BASE: Dec = dec!(100);

/// Wider than four times the largest cap, so a random walk over it fills any
/// side and keeps pushing once full, which is where the policy lives.
const GRID: u16 = 1_000;

/// The crate root is `src/mod.rs`, so proptest cannot find it to place a
/// regression file and warns on every run unless told where to record a case.
fn config() -> ProptestConfig {
    ProptestConfig {
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "tests/fuzz_book.proptest-regressions",
        ))),
        ..ProptestConfig::default()
    }
}

/// A price on the tick grid, `step` ticks above the base.
fn price(step: u16) -> Dec {
    BASE + TICK * Dec::from(step as i32)
}

/// Every invariant a side must keep, whatever has been done to it.
fn check(side: &OrderBookSide, cap: usize, desc: bool, what: &str) -> Result<(), TestCaseError> {
    prop_assert!(
        side.len() <= cap,
        "{what}: {} levels held past a cap of {cap}",
        side.len()
    );
    prop_assert_eq!(side.max_depth(), Some(cap), "{}: cap changed", what);

    let levels: Vec<_> = side.iter().copied().collect();
    prop_assert_eq!(
        levels.len(),
        side.len(),
        "{}: iter disagrees with len",
        what
    );

    for (rank, level) in levels.iter().enumerate() {
        prop_assert!(level.price.is_finite(), "{what}: non-finite price held");
        prop_assert!(level.amount.is_finite(), "{what}: non-finite amount held");
        prop_assert!(
            level.amount > Dec::ZERO,
            "{what}: {} held at rank {rank} with a non-positive amount",
            level.price
        );
        // best first, strictly: a repeated price would mean one level entered
        // twice rather than replacing the one already there
        if rank > 0 {
            let better = levels[rank - 1].price;
            match desc {
                true => prop_assert!(
                    better > level.price,
                    "{what}: {better} then {}",
                    level.price
                ),
                false => prop_assert!(
                    better < level.price,
                    "{what}: {better} then {}",
                    level.price
                ),
            }
        }
        // indexed and keyed lookup must agree with the iteration order
        prop_assert_eq!(side.at(rank), Some(level), "{}: at({}) differs", what, rank);
        prop_assert_eq!(
            side.find(level.price),
            Some(level),
            "{}: find differs",
            what
        );
    }

    prop_assert_eq!(
        side.best(),
        levels.first(),
        "{}: best is not the front",
        what
    );
    prop_assert_eq!(
        side.worst(),
        levels.last(),
        "{}: worst is not the back",
        what
    );
    prop_assert_eq!(side.at(side.len()), None, "{}: a level past the end", what);
    prop_assert_eq!(side.is_empty(), levels.is_empty(), "{}: is_empty", what);
    Ok(())
}

proptest! {
    #![proptest_config(config())]

    /// Whatever sequence of updates arrives, at every cap, the side stays
    /// within it and stays sorted. The same sequence is replayed at each cap,
    /// so a failure names the depth it needs rather than one it happens to hit.
    #[test]
    fn test_a_capped_side_never_exceeds_its_cap(
        ops in prop::collection::vec((0_u16..GRID, 0_u8..8), 1..600),
    ) {
        for cap in CAPS {
            let mut book = OrderBook::new(Some(cap));
            for (step, amount) in ops.iter().copied() {
                // a zero amount is a removal, which is the other half of a feed
                let amount = Dec::from(amount as i32);
                book.bids.set_price_amount(price(step), amount);
                book.asks.set_price_amount(price(step), amount);
                check(&book.bids, cap, true, "bids")?;
                check(&book.asks, cap, false, "asks")?;
            }
        }
    }

    /// Imbalance is a ratio of one side against the other, so it cannot leave
    /// `[-1, 1]` however the two sides are loaded. Its sign follows the heavier
    /// side, and the touch reading agrees with the one-level band.
    #[test]
    fn test_imbalance_stays_within_its_bounds(
        ops in prop::collection::vec((0_u16..GRID, 0_u8..8, any::<bool>()), 1..400),
    ) {
        for cap in CAPS {
            let mut book = OrderBook::new(Some(cap));
            for (step, amount, is_bid) in ops.iter().copied() {
                let (price, amount) = (price(step), Dec::from(amount as i32));
                match is_bid {
                    true => book.bids.set_price_amount(price, amount),
                    false => book.asks.set_price_amount(price, amount),
                };

                for value in [book.imbalance(), book.range_imbalance(0, cap)]
                    .into_iter()
                    .flatten()
                {
                    prop_assert!(value.is_finite(), "imbalance came back NaN");
                    prop_assert!(
                        value >= Dec::NEG_ONE && value <= Dec::ONE,
                        "imbalance {value} left [-1, 1] at cap {cap}"
                    );
                }

                // a band that asks for no levels has no reading, inverted or not
                prop_assert_eq!(
                    book.range_imbalance(cap, cap),
                    None,
                    "an empty band answered at cap {}",
                    cap
                );
                prop_assert_eq!(
                    book.range_imbalance(cap, 0),
                    None,
                    "an inverted band answered at cap {}",
                    cap
                );

                // the band of one level is the touch, by definition
                prop_assert_eq!(
                    book.range_imbalance(0, 1),
                    book.imbalance(),
                    "the one level band and the touch disagree at cap {}",
                    cap
                );

                // and the sign follows whichever side carries more
                if let (Some(value), Some(bid), Some(ask)) =
                    (book.imbalance(), book.bids.best(), book.asks.best())
                {
                    prop_assert_eq!(
                        value > Dec::ZERO,
                        bid.amount > ask.amount,
                        "{} against bid {} ask {}",
                        value,
                        bid.amount,
                        ask.amount
                    );
                }
            }
        }
    }

    /// A level the side holds carries the amount it was last given. Nothing
    /// invents a level, and nothing keeps one that was removed.
    #[test]
    fn test_a_held_level_carries_its_last_amount(
        ops in prop::collection::vec((0_u16..GRID, 0_u8..8), 1..600),
    ) {
        for cap in CAPS {
            let mut book = OrderBook::new(Some(cap));
            // what was last asked for at each price, whether or not it took
            let mut asked: BTreeMap<i128, Dec> = BTreeMap::new();
            for (step, amount) in ops.iter().copied() {
                let (price, amount) = (price(step), Dec::from(amount as i32));
                book.bids.set_price_amount(price, amount);
                asked.insert(price.into_raw(), amount);

                for level in book.bids.iter() {
                    let last = asked.get(&level.price.into_raw());
                    prop_assert_eq!(
                        last,
                        Some(&level.amount),
                        "{} held at {} against a last request of {:?}",
                        level.amount,
                        level.price,
                        last
                    );
                }
            }
        }
    }

}

/// A side filled past its cap holds the best levels it was offered while
/// filling, and nothing worse. Prices arrive in a shuffled order so the result
/// cannot come from the order they were given in.
#[test]
fn test_filling_past_the_cap_keeps_the_best() {
    for cap in CAPS {
        let offered = cap * 3;
        let mut book = OrderBook::new(Some(cap));
        // a stride coprime with the span walks every price once, in an order
        // that is neither ascending nor descending
        let stride = 7_u16;
        for n in 0..offered as u16 {
            let step = (n * stride) % offered as u16;
            book.bids.set_price_amount(price(step), dec!(1));
        }
        check(&book.bids, cap, true, "filled").expect("a filled side");
        assert_eq!(book.bids.len(), cap, "a side offered {offered} levels");

        // every price the side kept beats every price it did not
        let held: Vec<Dec> = book.bids.iter().map(|level| level.price).collect();
        let worst_held = *held.last().expect("a full side");
        for n in 0..offered as u16 {
            let candidate = price(n);
            if candidate > worst_held {
                assert!(
                    book.bids.find(candidate).is_some(),
                    "{candidate} beats the worst held {worst_held} and is missing at cap {cap}"
                );
            }
        }
    }
}

/// A better price evicts the worst; a worse one is refused outright. Written
/// out at each cap, because this is the whole of what a cap does and it should
/// be readable without a generator.
#[test]
fn test_a_full_side_evicts_the_worst_and_refuses_the_rest() {
    for cap in CAPS {
        let mut book = OrderBook::new(Some(cap));
        // fill exactly, best first, so the worst sits at step 0
        for step in (0..cap as u16).rev() {
            book.bids.set_price_amount(price(step), dec!(1));
        }
        assert_eq!(book.bids.len(), cap);
        assert_eq!(book.bids.best_price(), Some(price(cap as u16 - 1)));
        assert_eq!(book.bids.worst_price(), Some(price(0)));

        // a price better than the best goes in, and the worst leaves to make
        // room: the side is still exactly full
        let better = price(cap as u16);
        assert_eq!(book.bids.set_price_amount(better, dec!(2)), None);
        assert_eq!(book.bids.len(), cap, "cap {cap} grew on an eviction");
        assert_eq!(book.bids.best_price(), Some(better));
        assert_eq!(book.bids.worst_price(), Some(price(1)), "cap {cap}");
        assert!(
            book.bids.find(price(0)).is_none(),
            "cap {cap} kept the evicted"
        );

        // a price worse than the worst held is refused, and the side is
        // untouched: not merely trimmed back, never entered
        let worse = price(0) - TICK;
        assert_eq!(book.bids.set_price_amount(worse, dec!(3)), None);
        assert_eq!(book.bids.len(), cap, "cap {cap} grew on a refusal");
        assert!(
            book.bids.find(worse).is_none(),
            "cap {cap} kept a refused level"
        );
        assert_eq!(book.bids.worst_price(), Some(price(1)), "cap {cap}");

        // and a removal frees exactly one place, which the next level takes
        assert!(book.bids.set_price_amount(better, Dec::ZERO).is_some());
        assert_eq!(book.bids.len(), cap - 1, "cap {cap} after a removal");
        assert_eq!(book.bids.set_price_amount(worse, dec!(3)), None);
        assert_eq!(book.bids.len(), cap, "cap {cap} refilled");
        assert_eq!(book.bids.worst_price(), Some(worse), "cap {cap}");
    }
}

/// `trim` is the cap applied after the fact, and leaves the same shape.
#[test]
fn test_trimming_to_each_cap_keeps_the_best() {
    for cap in CAPS {
        let mut book = OrderBook::new(None);
        for step in 0..(cap as u16 * 2) {
            book.bids.set_price_amount(price(step), dec!(1));
            book.asks.set_price_amount(price(step), dec!(1));
        }
        let best_bid = book.bids.best_price();
        let best_ask = book.asks.best_price();

        book.bids.trim(cap);
        book.asks.trim(cap);
        assert_eq!(book.bids.len(), cap);
        assert_eq!(book.asks.len(), cap);
        assert_eq!(
            book.bids.best_price(),
            best_bid,
            "cap {cap} lost the best bid"
        );
        assert_eq!(
            book.asks.best_price(),
            best_ask,
            "cap {cap} lost the best ask"
        );
        // trimming to more than is held is a no-op, not a truncation
        book.bids.trim(cap * 10);
        assert_eq!(book.bids.len(), cap, "cap {cap} shrank on a wider trim");
    }
}
