use super::PriceAmount;
use crate::Dec;

const DEFAULT_CAPACITY: usize = 64;

/// One side of a level 2 book.
///
/// Levels are kept best first — descending for bids, ascending for asks — so
/// the best level, the n-th level and the depth cut are all index arithmetic.
#[derive(Clone, Debug)]
pub struct OrderBookSide {
    levels: Vec<PriceAmount>,
    desc: bool,
    max_depth: Option<usize>,
    total_amount: Dec,
}

/// A level 2 order book.
///
/// Designed to be efficient for high-frequency trading scenarios.
#[derive(Clone, Debug)]
pub struct OrderBook {
    /// The bids side of the order book.
    pub bids: OrderBookSide,
    /// The asks side of the order book.
    pub asks: OrderBookSide,
}

/// The best bid and ask: the smallest snapshot of a book that still prices a
/// trade, and what a top-of-book feed carries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OrderBookTop {
    /// The best bid.
    pub bid: PriceAmount,
    /// The best ask.
    pub ask: PriceAmount,
}

/// A batch of level updates to apply to a book.
#[derive(Clone, Debug, Default)]
pub struct OrderBookDiff {
    /// Bid levels to set.
    pub bids: Vec<PriceAmount>,
    /// Ask levels to set.
    pub asks: Vec<PriceAmount>,
}

impl OrderBookSide {
    fn bids(max_depth: Option<usize>) -> Self {
        Self::new(true, max_depth)
    }

    fn asks(max_depth: Option<usize>) -> Self {
        Self::new(false, max_depth)
    }

    fn new(desc: bool, max_depth: Option<usize>) -> Self {
        // max_depth is a ceiling, not a forecast: a side capped at ten
        // thousand levels usually holds a handful, so reserving the cap would
        // allocate for a book that never arrives — and `usize::MAX`, which is
        // a legal cap meaning "no limit", aborts the process outright
        let capacity = max_depth.unwrap_or(DEFAULT_CAPACITY).min(DEFAULT_CAPACITY);
        Self {
            levels: Vec::with_capacity(capacity),
            desc,
            max_depth,
            total_amount: Dec::ZERO,
        }
    }

    /// The number of levels held.
    pub fn len(&self) -> usize {
        self.levels.len()
    }

    /// Whether the side holds no levels.
    pub fn is_empty(&self) -> bool {
        self.levels.is_empty()
    }

    /// The level cap this side was built with, or `None` when uncapped.
    pub fn max_depth(&self) -> Option<usize> {
        self.max_depth
    }

    /// Iterate every level, best first.
    pub fn iter(&self) -> std::slice::Iter<'_, PriceAmount> {
        self.levels.iter()
    }

    /// The level at exactly `price`, if the side holds one.
    ///
    /// Keyed by price, where [`OrderBookSide::at`] is keyed by depth.
    pub fn find(&self, price: Dec) -> Option<&PriceAmount> {
        self.search(price).ok().map(|index| &self.levels[index])
    }

    /// The best level: highest bid, lowest ask.
    pub fn best(&self) -> Option<&PriceAmount> {
        self.levels.first()
    }

    /// The price of the best level.
    pub fn best_price(&self) -> Option<Dec> {
        self.best().map(|level| level.price)
    }

    /// The worst level held, which is the deepest one, not the worst possible.
    pub fn worst(&self) -> Option<&PriceAmount> {
        self.levels.last()
    }

    /// The price of the worst level held.
    pub fn worst_price(&self) -> Option<Dec> {
        self.worst().map(|level| level.price)
    }

    /// The n-th level from the best, counting from zero.
    pub fn at(&self, n: usize) -> Option<&PriceAmount> {
        self.levels.get(n)
    }

    /// The price of the n-th level from the best.
    pub fn price_at(&self, n: usize) -> Option<Dec> {
        self.at(n).map(|level| level.price)
    }

    /// Set a level, returning the amount it replaced. A zero amount removes it.
    ///
    /// A level failing [`PriceAmount::is_valid`] is rejected and the book left
    /// untouched. `None` therefore covers four cases: the level was inserted,
    /// it was rejected as invalid, it was refused for sitting past
    /// `max_depth`, or a zero amount asked to remove a price that was absent.
    pub fn set(&mut self, entry: PriceAmount) -> Option<Dec> {
        if !entry.is_valid() {
            return None;
        }
        match self.search(entry.price) {
            Ok(index) => {
                let previous = self.levels[index].amount;
                self.discount(previous);
                if entry.amount.is_zero() {
                    self.levels.remove(index);
                } else {
                    self.levels[index].amount = entry.amount;
                    self.accrue(entry.amount);
                }
                Some(previous)
            }
            Err(index) => {
                if entry.amount.is_zero() || self.max_depth.is_some_and(|depth| index >= depth) {
                    return None;
                }
                self.levels.insert(index, entry);
                self.accrue(entry.amount);
                if let Some(depth) = self.max_depth {
                    self.trim(depth);
                }
                None
            }
        }
    }

    /// [`OrderBookSide::set`] taking the price and amount separately.
    pub fn set_price_amount(&mut self, price: Dec, amount: Dec) -> Option<Dec> {
        self.set(PriceAmount { price, amount })
    }

    /// Retain at most `depth` levels, dropping the worst ones beyond that.
    pub fn trim(&mut self, depth: usize) {
        if self.levels.len() <= depth {
            return;
        }
        let mut amount = Dec::ZERO;
        for level in self.levels.drain(depth..) {
            amount += level.amount;
        }
        self.total_amount -= amount;
    }

    /// Iterate levels within `[low, high]` in best-first order.
    ///
    /// An empty iterator when no level falls inside, `low` above `high`
    /// included.
    pub fn range(&self, low: Dec, high: Dec) -> std::slice::Iter<'_, PriceAmount> {
        let (start, end) = match self.desc {
            true => (
                self.levels.partition_point(|level| level.price > high),
                self.levels.partition_point(|level| level.price >= low),
            ),
            false => (
                self.levels.partition_point(|level| level.price < low),
                self.levels.partition_point(|level| level.price <= high),
            ),
        };
        // the two partition points cross rather than meet when `low` sits
        // above `high`, and the slice would panic on a backwards range
        self.levels[start..end.max(start)].iter()
    }

    /// Get the volume up to a given level in the orderbook side
    pub fn volume_at(&self, level: usize) -> Option<Dec> {
        self.levels
            .iter()
            .take(level)
            .map(|entry| entry.amount)
            .reduce(|acc, x| acc + x)
    }

    /// Volume weighted price paid to fill `quantity`, or `None` if the side is
    /// too thin.
    pub fn price_for_quantity(&self, quantity: f64) -> Option<f64> {
        if quantity <= 0.0 {
            return None;
        }
        let mut accumulated = 0.0;
        let mut notional = 0.0;
        for entry in self.levels.iter() {
            let amount = (quantity - accumulated).min(entry.amount.to_f64());
            notional += amount * entry.price.to_f64();
            accumulated += amount;
            if accumulated >= quantity {
                return Some(notional / accumulated);
            }
        }
        None
    }

    /// Mean level amount, or `None` when the side is empty.
    pub fn amount_mean(&self) -> Option<f64> {
        match self.levels.len() {
            0 => None,
            count => Some(self.total_amount.to_f64() / count as f64),
        }
    }

    /// Population standard deviation of the level amounts, or `None` when the
    /// side is empty.
    ///
    /// Computed from the levels rather than from a running sum of squares. A
    /// running one drifts: adding and removing a level far larger than the
    /// rest cancels badly enough to report no dispersion at all on a book that
    /// has some, and the clamp that hid it turned corruption into a plausible
    /// number. The mean comes from the exact `Dec` total, so only the
    /// deviations are floating point, and their squares cannot go negative.
    pub fn amount_std_dev(&self) -> Option<f64> {
        let count = self.levels.len();
        if count == 0 {
            return None;
        }
        let n = count as f64;
        let mean = self.total_amount.to_f64() / n;
        let variance = self
            .levels
            .iter()
            .map(|level| {
                let deviation = level.amount.to_f64() - mean;
                deviation * deviation
            })
            .sum::<f64>()
            / n;
        Some(variance.sqrt())
    }

    #[inline]
    fn search(&self, price: Dec) -> Result<usize, usize> {
        match self.desc {
            true => self
                .levels
                .binary_search_by(|level| price.cmp(&level.price)),
            false => self
                .levels
                .binary_search_by(|level| level.price.cmp(&price)),
        }
    }

    #[inline]
    fn accrue(&mut self, amount: Dec) {
        self.total_amount += amount;
    }

    #[inline]
    fn discount(&mut self, amount: Dec) {
        self.total_amount -= amount;
    }
}

impl Default for OrderBook {
    fn default() -> Self {
        Self::new(None)
    }
}

impl OrderBook {
    /// An empty book, each side capped at `max_depth` levels, or uncapped on
    /// `None`.
    pub fn new(max_depth: Option<usize>) -> Self {
        Self {
            bids: OrderBookSide::bids(max_depth),
            asks: OrderBookSide::asks(max_depth),
        }
    }

    /// The level cap both sides were built with.
    pub fn max_depth(&self) -> Option<usize> {
        self.bids.max_depth()
    }

    /// O(1) operation to get the mid price of the order book.
    ///
    /// Returns `None` if either the bid or ask side is empty.
    pub fn mid_price(&self) -> Option<Dec> {
        match (self.bids.best_price(), self.asks.best_price()) {
            (Some(bid), Some(ask)) => Some(bid.midpoint(ask)),
            _ => None,
        }
    }

    /// Best ask less best bid, or `None` if either side is empty.
    ///
    /// Negative when the book is crossed, which a feed can produce.
    pub fn spread(&self) -> Option<Dec> {
        match (self.bids.best_price(), self.asks.best_price()) {
            (Some(bid), Some(ask)) => Some(ask - bid),
            _ => None,
        }
    }

    /// Apply every level in `diff`, each as [`OrderBookSide::set`] does.
    pub fn apply_diff(&mut self, diff: &OrderBookDiff) {
        for bid in diff.bids.iter().copied() {
            self.bids.set(bid);
        }
        for ask in diff.asks.iter().copied() {
            self.asks.set(ask);
        }
    }
}

impl From<OrderBookTop> for OrderBook {
    fn from(top: OrderBookTop) -> Self {
        let mut order_book = OrderBook::default();
        order_book.bids.set(top.bid);
        order_book.asks.set(top.ask);
        order_book
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::dec;
    use crate::testing::{OrderBookBuilder, RandomWalk, Side};

    fn bids(levels: &[(Dec, Dec)], max_depth: Option<usize>) -> OrderBookSide {
        let mut side = OrderBookSide::bids(max_depth);
        for (price, amount) in levels.iter().copied() {
            side.set_price_amount(price, amount);
        }
        side
    }

    fn asks(levels: &[(Dec, Dec)], max_depth: Option<usize>) -> OrderBookSide {
        let mut side = OrderBookSide::asks(max_depth);
        for (price, amount) in levels.iter().copied() {
            side.set_price_amount(price, amount);
        }
        side
    }

    #[test]
    fn test_bids_are_ordered_best_first() {
        let side = bids(
            &[
                (dec!(99), dec!(1)),
                (dec!(101), dec!(2)),
                (dec!(100), dec!(3)),
            ],
            None,
        );
        let prices: Vec<Dec> = side.iter().map(|level| level.price).collect();
        assert_eq!(prices, vec![dec!(101), dec!(100), dec!(99)]);
        assert_eq!(side.best_price(), Some(dec!(101)));
        assert_eq!(side.worst_price(), Some(dec!(99)));
        assert_eq!(side.price_at(1), Some(dec!(100)));
    }

    #[test]
    fn test_asks_are_ordered_best_first() {
        let side = asks(
            &[
                (dec!(101), dec!(1)),
                (dec!(99), dec!(2)),
                (dec!(100), dec!(3)),
            ],
            None,
        );
        let prices: Vec<Dec> = side.iter().map(|level| level.price).collect();
        assert_eq!(prices, vec![dec!(99), dec!(100), dec!(101)]);
        assert_eq!(side.best_price(), Some(dec!(99)));
        assert_eq!(side.worst_price(), Some(dec!(101)));
    }

    #[test]
    fn test_set_replaces_and_removes() {
        let mut side = bids(&[(dec!(100), dec!(1)), (dec!(99), dec!(2))], None);
        assert_eq!(side.set_price_amount(dec!(100), dec!(5)), Some(dec!(1)));
        assert_eq!(side.volume_at(side.len()), Some(dec!(7)));
        assert_eq!(side.set_price_amount(dec!(100), dec!(0)), Some(dec!(5)));
        assert_eq!(side.len(), 1);
        assert_eq!(side.volume_at(side.len()), Some(dec!(2)));
        assert_eq!(side.find(dec!(100)), None);
        assert_eq!(side.set_price_amount(dec!(98), dec!(0)), None);
    }

    #[test]
    fn test_is_valid_rejects_what_a_book_cannot_hold() {
        let valid = PriceAmount {
            price: dec!(100),
            amount: dec!(1),
        };
        assert!(valid.is_valid());
        assert!(
            PriceAmount {
                price: dec!(100),
                amount: Dec::ZERO,
            }
            .is_valid(),
            "a zero amount is the removal of a level, not an invalid one"
        );
        for bad in [
            PriceAmount {
                price: Dec::NAN,
                amount: dec!(1),
            },
            PriceAmount {
                price: dec!(100),
                amount: Dec::NAN,
            },
            PriceAmount {
                price: dec!(100),
                amount: dec!(-1),
            },
        ] {
            assert!(!bad.is_valid(), "{bad:?}");
        }
    }

    #[test]
    fn test_an_invalid_level_leaves_the_book_untouched() {
        let mut side = bids(&[(dec!(100), dec!(1)), (dec!(99), dec!(2))], None);
        for bad in [
            (Dec::NAN, dec!(5)),
            (dec!(101), Dec::NAN),
            (dec!(101), dec!(-5)),
        ] {
            assert_eq!(side.set_price_amount(bad.0, bad.1), None);
        }
        let prices: Vec<Dec> = side.iter().map(|level| level.price).collect();
        assert_eq!(prices, vec![dec!(100), dec!(99)]);
        // a NaN amount can never be taken back out of the total, so the test
        // that matters is that it never went in
        assert_eq!(side.volume_at(side.len()), Some(dec!(3)));
        assert_eq!(side.amount_mean(), Some(1.5));
    }

    #[test]
    fn test_range_with_low_above_high_is_empty() {
        let side = asks(&[(dec!(100), dec!(1)), (dec!(101), dec!(1))], None);
        assert_eq!(side.range(dec!(101), dec!(100)).count(), 0);
        let side = bids(&[(dec!(101), dec!(1)), (dec!(100), dec!(1))], None);
        assert_eq!(side.range(dec!(101), dec!(100)).count(), 0);
    }

    #[test]
    fn test_a_large_max_depth_does_not_reserve_it() {
        // usize::MAX is a legal cap meaning "no limit"; reserving it aborts
        let mut book = OrderBook::new(Some(usize::MAX));
        book.asks.set_price_amount(dec!(100), dec!(1));
        assert_eq!(book.asks.len(), 1);
        assert_eq!(book.max_depth(), Some(usize::MAX));
    }

    #[test]
    fn test_max_depth_drops_worst_levels() {
        let mut side = bids(
            &[
                (dec!(100), dec!(1)),
                (dec!(99), dec!(2)),
                (dec!(98), dec!(3)),
            ],
            Some(2),
        );
        assert_eq!(side.len(), 2);
        assert_eq!(side.volume_at(side.len()), Some(dec!(3)));
        // a better price evicts the worst level
        side.set_price_amount(dec!(101), dec!(4));
        assert_eq!(side.len(), 2);
        assert_eq!(side.best_price(), Some(dec!(101)));
        assert_eq!(side.worst_price(), Some(dec!(100)));
        assert_eq!(side.volume_at(side.len()), Some(dec!(5)));
        // a worse price is ignored
        assert_eq!(side.set_price_amount(dec!(97), dec!(9)), None);
        assert_eq!(side.len(), 2);
    }

    #[test]
    fn test_trim() {
        let mut side = asks(
            &[
                (dec!(100), dec!(1)),
                (dec!(101), dec!(2)),
                (dec!(102), dec!(3)),
            ],
            None,
        );
        side.trim(1);
        assert_eq!(side.len(), 1);
        assert_eq!(side.volume_at(side.len()), Some(dec!(1)));
        assert_eq!(side.amount_std_dev(), Some(0.0));
    }

    #[test]
    fn test_volume_and_stats() {
        let side = asks(
            &[
                (dec!(100), dec!(1)),
                (dec!(101), dec!(2)),
                (dec!(102), dec!(3)),
            ],
            None,
        );
        assert_eq!(side.volume_at(2), Some(dec!(3)));
        assert_eq!(side.volume_at(10), Some(dec!(6)));
        assert_eq!(side.amount_mean(), Some(2.0));
        let std_dev = side.amount_std_dev().expect("non empty side");
        assert!((std_dev - (2.0_f64 / 3.0).sqrt()).abs() < 1e-12);
    }

    #[test]
    fn test_price_for_quantity() {
        let side = asks(&[(dec!(100), dec!(1)), (dec!(102), dec!(1))], None);
        assert_eq!(side.price_for_quantity(1.0), Some(100.0));
        assert_eq!(side.price_for_quantity(2.0), Some(101.0));
        assert_eq!(side.price_for_quantity(3.0), None);
        assert_eq!(side.price_for_quantity(0.0), None);
    }

    #[test]
    fn test_apply_diff() {
        let mut book = OrderBook::new(None);
        book.apply_diff(&OrderBookDiff {
            bids: vec![PriceAmount {
                price: dec!(99),
                amount: dec!(1),
            }],
            asks: vec![PriceAmount {
                price: dec!(101),
                amount: dec!(2),
            }],
        });
        assert_eq!(book.mid_price(), Some(dec!(100)));
        assert_eq!(book.spread(), Some(dec!(2)));
        book.apply_diff(&OrderBookDiff {
            bids: vec![PriceAmount {
                price: dec!(99),
                amount: dec!(0),
            }],
            ..Default::default()
        });
        assert!(book.bids.is_empty());
        assert_eq!(book.mid_price(), None);
    }

    #[test]
    fn test_order_book_from_top() {
        let top = OrderBookTop {
            bid: PriceAmount {
                price: dec!(99),
                amount: dec!(1),
            },
            ask: PriceAmount {
                price: dec!(101),
                amount: dec!(2),
            },
        };
        let book = OrderBook::from(top);
        assert_eq!(book.bids.best_price(), Some(dec!(99)));
        assert_eq!(book.asks.best_price(), Some(dec!(101)));
        assert_eq!(book.mid_price(), Some(dec!(100)));
    }

    /// After a simulation step the book must not be crossed: best bid < best ask,
    /// and spread() must equal ask - bid.
    #[test]
    fn test_simulate_book_never_crossed() {
        let mut builder = OrderBookBuilder::new()
            .with_tick_size(dec!(0.01))
            .with_spread(dec!(0.02))
            .with_amount(dec!(10));
        let prices = RandomWalk::new(100).lognormal(100.0, 0.0, 0.2).unwrap();
        for mid in prices {
            let (book, _) = builder.simulate(Dec::from_f64(mid).unwrap(), 5);
            let bid = book.bids.best_price().expect("bids must be non-empty");
            let ask = book.asks.best_price().expect("asks must be non-empty");
            assert!(
                bid < ask,
                "book crossed after simulate: bid={bid} ask={ask}"
            );
            assert_eq!(book.spread(), Some(ask - bid));
            assert_eq!(
                book.bids.len(),
                5,
                "expected 5 bid levels, got {}",
                book.bids.len()
            );
            assert_eq!(
                book.asks.len(),
                5,
                "expected 5 ask levels, got {}",
                book.asks.len()
            );
        }
    }

    /// All amounts in the book must be strictly positive after every simulate step —
    /// zero-amount levels are removals and must never persist.
    #[test]
    fn test_simulate_all_amounts_positive() {
        let mut builder = OrderBookBuilder::new()
            .with_tick_size(dec!(0.01))
            .with_spread(dec!(0.02))
            .with_amount(dec!(10));
        let prices = RandomWalk::new(100).lognormal(100.0, 0.0, 0.2).unwrap();
        for mid in prices {
            let (book, _) = builder.simulate(Dec::from_f64(mid).unwrap(), 20);
            for level in book.bids.iter() {
                assert!(
                    level.amount > Dec::ZERO,
                    "bid level at {} has non-positive amount {}",
                    level.price,
                    level.amount
                );
            }
            for level in book.asks.iter() {
                assert!(
                    level.amount > Dec::ZERO,
                    "ask level at {} has non-positive amount {}",
                    level.price,
                    level.amount
                );
            }
        }
    }

    /// All prices must be multiples of tick_size and all amounts multiples of lot_size.
    #[test]
    fn test_simulate_prices_and_amounts_aligned() {
        let tick = dec!(0.01);
        let lot = dec!(0.01);
        let mut builder = OrderBookBuilder::new()
            .with_tick_size(tick)
            .with_lot_size(lot)
            .with_spread(dec!(0.02))
            .with_amount(dec!(10));
        let prices = RandomWalk::new(100).lognormal(100.0, 0.0, 0.2).unwrap();
        for mid in prices {
            let (book, _) = builder.simulate(Dec::from_f64(mid).unwrap(), 20);
            for level in book.bids.iter().chain(book.asks.iter()) {
                assert_eq!(
                    level.price.into_raw() % tick.into_raw(),
                    0,
                    "price {} is not a multiple of tick {}",
                    level.price,
                    tick
                );
                assert_eq!(
                    level.amount.into_raw() % lot.into_raw(),
                    0,
                    "amount {} is not a multiple of lot {}",
                    level.amount,
                    lot
                );
            }
        }
    }

    /// Trades returned in events must correspond to levels that crossed the new spread:
    /// bid-side trades (Side::Sell) have price > new bid, ask-side trades (Side::Buy) have price < new ask.
    #[test]
    fn test_simulate_trades_are_crossed_levels() {
        let mut builder = OrderBookBuilder::new()
            .with_tick_size(dec!(0.01))
            .with_spread(dec!(0.02))
            .with_amount(dec!(10));
        let prices = RandomWalk::new(100).lognormal(100.0, 0.0, 0.2).unwrap();
        for mid in prices {
            let (book, events) = builder.simulate(Dec::from_f64(mid).unwrap(), 20);
            let bid = book.bids.best_price().expect("bids must be non-empty");
            let ask = book.asks.best_price().expect("asks must be non-empty");
            for trade in &events.trades {
                match trade.side {
                    Side::Sell => assert!(
                        trade.price_amount.price > bid,
                        "bid-side trade at {} is not above new bid {}",
                        trade.price_amount.price,
                        bid
                    ),
                    Side::Buy => assert!(
                        trade.price_amount.price < ask,
                        "ask-side trade at {} is not below new ask {}",
                        trade.price_amount.price,
                        ask
                    ),
                }
            }
        }
    }

    /// Every new order returned in events must be present in the book after simulate.
    #[test]
    fn test_simulate_new_orders_in_book() {
        let mut builder = OrderBookBuilder::new()
            .with_tick_size(dec!(0.01))
            .with_spread(dec!(0.02))
            .with_amount(dec!(10));
        let prices = RandomWalk::new(100).lognormal(100.0, 0.0, 0.2).unwrap();
        for mid in prices {
            let (book, events) = builder.simulate(Dec::from_f64(mid).unwrap(), 20);
            for order in &events.new_orders {
                match order.side {
                    Side::Buy => assert!(
                        book.bids
                            .iter()
                            .any(|level| level.price == order.price_amount.price),
                        "new bid at {} not found in book",
                        order.price_amount.price
                    ),
                    Side::Sell => assert!(
                        book.asks
                            .iter()
                            .any(|level| level.price == order.price_amount.price),
                        "new ask at {} not found in book",
                        order.price_amount.price
                    ),
                }
            }
        }
    }

    /// volume_at properties: None at 0, equals best amount at 1,
    /// monotonically non-decreasing, and equals total at full depth.
    #[test]
    fn test_simulate_volume_at_properties() {
        let mut builder = OrderBookBuilder::new()
            .with_tick_size(dec!(0.01))
            .with_spread(dec!(0.02))
            .with_amount(dec!(10));
        let prices = RandomWalk::new(100).lognormal(100.0, 0.0, 0.2).unwrap();
        for mid in prices {
            let (book, _) = builder.simulate(Dec::from_f64(mid).unwrap(), 20);
            for side in [&book.bids, &book.asks] {
                // volume_at(0) is always None
                assert_eq!(side.volume_at(0), None);

                // volume_at(1) equals the best level amount
                let best_amount = side.best().unwrap().amount;
                assert_eq!(side.volume_at(1), Some(best_amount));

                // volume_at is monotonically non-decreasing
                let mut prev = Dec::ZERO;
                for n in 1..=side.len() {
                    let vol = side.volume_at(n).unwrap();
                    assert!(
                        vol >= prev,
                        "volume_at({n})={vol} < volume_at({})={prev}",
                        n - 1
                    );
                    prev = vol;
                }

                // volume_at(len) equals sum of all amounts
                let total: Dec = side.iter().map(|level| level.amount).sum();
                assert_eq!(side.volume_at(side.len()), Some(total));
            }
        }
    }

    #[test]
    fn test_range_empty_returns_nothing() {
        let book = OrderBookBuilder::new()
            .with_ask(dec!(100), dec!(1))
            .with_ask(dec!(101), dec!(1))
            .build();
        let result: Vec<_> = book.asks.range(dec!(200), dec!(300)).collect();
        assert!(result.is_empty());
    }

    #[test]
    fn test_range_asks_ascending_best_first() {
        let book = OrderBookBuilder::new()
            .with_ask(dec!(100), dec!(1))
            .with_ask(dec!(101), dec!(2))
            .with_ask(dec!(102), dec!(3))
            .with_ask(dec!(103), dec!(4))
            .build();
        let prices: Vec<_> = book
            .asks
            .range(dec!(100), dec!(102))
            .map(|l| l.price)
            .collect();
        assert_eq!(prices, vec![dec!(100), dec!(101), dec!(102)]);
    }

    #[test]
    fn test_range_bids_descending_best_first() {
        let book = OrderBookBuilder::new()
            .with_bid(dec!(100), dec!(1))
            .with_bid(dec!(101), dec!(2))
            .with_bid(dec!(102), dec!(3))
            .with_bid(dec!(103), dec!(4))
            .build();
        let prices: Vec<_> = book
            .bids
            .range(dec!(101), dec!(103))
            .map(|l| l.price)
            .collect();
        assert_eq!(prices, vec![dec!(103), dec!(102), dec!(101)]);
    }

    #[test]
    fn test_range_bounds_not_in_book() {
        let book = OrderBookBuilder::new()
            .with_ask(dec!(99), dec!(1))
            .with_ask(dec!(100), dec!(2))
            .with_ask(dec!(101), dec!(3))
            .with_ask(dec!(102), dec!(4))
            .build();
        let prices: Vec<_> = book
            .asks
            .range(dec!(99.5), dec!(101.5))
            .map(|l| l.price)
            .collect();
        assert_eq!(prices, vec![dec!(100), dec!(101)]);
    }

    #[test]
    fn test_amount_stats_after_trim() {
        // amounts 1,2,3,4,5 at bids 101..105 — trim to 3 keeps the 3 best (highest) bids
        // remaining amounts: 3, 4, 5 → mean=4, variance=2/3, std_dev=sqrt(2/3)
        let mut book = OrderBookBuilder::new()
            .with_bid(dec!(101), dec!(1))
            .with_bid(dec!(102), dec!(2))
            .with_bid(dec!(103), dec!(3))
            .with_bid(dec!(104), dec!(4))
            .with_bid(dec!(105), dec!(5))
            .build();
        book.bids.trim(3);
        assert_eq!(book.bids.len(), 3);
        assert_eq!(book.bids.amount_mean(), Some(4.0));
        let std_dev = book.bids.amount_std_dev().unwrap();
        let expected = (2.0f64 / 3.0).sqrt();
        let diff = (std_dev - expected).abs();
        assert!(diff < 1e-7, "std_dev={std_dev} expected={expected}");
    }

    #[test]
    fn test_amount_stats_empty() {
        let book = OrderBookBuilder::new().build();
        assert_eq!(book.bids.amount_mean(), None);
        assert_eq!(book.bids.amount_std_dev(), None);
    }

    #[test]
    fn test_amount_stats_single_level() {
        let book = OrderBookBuilder::new().with_bid(dec!(100), dec!(5)).build();
        assert_eq!(book.bids.amount_mean(), Some(5.0));
        assert_eq!(book.bids.amount_std_dev(), Some(0.0));
    }

    #[test]
    fn test_amount_stats_multiple_levels() {
        // amounts 1, 2, 3, 4, 5 → mean=3, variance=2, std_dev=sqrt(2)
        let book = OrderBookBuilder::new()
            .with_bid(dec!(101), dec!(1))
            .with_bid(dec!(102), dec!(2))
            .with_bid(dec!(103), dec!(3))
            .with_bid(dec!(104), dec!(4))
            .with_bid(dec!(105), dec!(5))
            .build();
        assert_eq!(book.bids.amount_mean(), Some(3.0));
        let std_dev = book.bids.amount_std_dev().unwrap();
        let expected = 2.0f64.sqrt();
        let diff = (std_dev - expected).abs();
        assert!(diff < 1e-7, "std_dev={std_dev} expected={expected}");
    }

    #[test]
    fn test_amount_stats_after_removal() {
        // amounts 2, 4, 6 → remove 2 → remaining 4, 6 → mean=5, variance=1, std_dev=1
        let mut book = OrderBookBuilder::new()
            .with_bid(dec!(100), dec!(2))
            .with_bid(dec!(101), dec!(4))
            .with_bid(dec!(102), dec!(6))
            .build();
        book.bids.set_price_amount(dec!(100), Dec::ZERO);
        assert_eq!(book.bids.amount_mean(), Some(5.0));
        assert_eq!(book.bids.amount_std_dev(), Some(1.0));
    }

    #[test]
    fn test_amount_stats_after_update() {
        // amounts 2, 4 → update 2→6 → amounts 6, 4 → mean=5, variance=1, std_dev=1
        let mut book = OrderBookBuilder::new()
            .with_bid(dec!(100), dec!(2))
            .with_bid(dec!(101), dec!(4))
            .build();
        book.bids.set_price_amount(dec!(100), dec!(6));
        assert_eq!(book.bids.amount_mean(), Some(5.0));
        assert_eq!(book.bids.amount_std_dev(), Some(1.0));
    }
}
