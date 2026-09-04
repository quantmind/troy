//! Building and simulating order books, for tests and for downstream crates
//! that enable the `testing` feature.
//!
//! The crate denies `panic!` and `expect` because a fault on a trading hot
//! path should reach the caller rather than abort it. Neither applies here: a
//! builder given a negative tick size is a mistake in the test itself, best
//! reported loudly and at once, and none of this ships in a default build.
#![allow(clippy::panic, clippy::expect_used)]

use crate::{Dec, OrderBook, PriceAmount, dec};
use rand::{RngExt, SeedableRng, rngs::SmallRng};
use rand_distr::{Distribution, Exp, Normal, NormalError};

/// Which side of the book a level, order or trade sits on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    /// Buying: the bid side.
    Buy,
    /// Selling: the ask side.
    Sell,
}

/// A level tagged with the side it came from, which a book event needs and a
/// level on its own does not carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PriceAmountSide {
    /// The price and amount.
    pub price_amount: PriceAmount,
    /// The side the level sat on.
    pub side: Side,
}

#[derive(Debug)]
#[allow(dead_code)] // stub: the fields are set, the builder methods are not written yet
/// Builder for an [`OrderBookTop`](crate::OrderBookTop).
pub struct OrderBookTopBuilder {
    mid: Dec,
    bid_spread: Dec,
    ask_spread: Dec,
    bid_amount: Dec,
    ask_amount: Dec,
    sequence_number: u64,
}

/// Builder for a full L2 [`OrderBook`] with arbitrary bid and ask levels and
/// simulated market activity.
#[derive(Debug)]
pub struct OrderBookBuilder {
    book: OrderBook,
    tick_size: Dec,
    lot_size: Dec,
    spread: Dec,
    amount: Dec,
    arrival_rate: Dec,
    rng: SmallRng,
}

#[derive(Default, Debug, Clone)]
/// What a simulated step did to the book.
pub struct OrderBookEvents {
    /// Levels that crossed the new spread and traded.
    pub trades: Vec<PriceAmountSide>,
    /// Levels withdrawn before the step.
    pub cancels: Vec<PriceAmountSide>,
    /// Levels added to refill the book.
    pub new_orders: Vec<PriceAmountSide>,
}

impl Default for OrderBookBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl OrderBookBuilder {
    /// A builder with an empty book and default tick, lot, spread and amount.
    pub fn new() -> Self {
        Self {
            book: OrderBook::default(),
            tick_size: dec!(0.01),
            lot_size: dec!(0.01),
            spread: dec!(0.01),
            amount: dec!(10),
            arrival_rate: Dec::ONE,
            rng: SmallRng::seed_from_u64(0),
        }
    }

    /// Seed the generator, so a simulation repeats exactly.
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.rng = SmallRng::seed_from_u64(seed);
        self
    }

    /// Add a bid level.
    pub fn with_bid(mut self, price: Dec, amount: Dec) -> Self {
        self.book.bids.set_price_amount(price, amount);
        self
    }

    /// Add an ask level.
    pub fn with_ask(mut self, price: Dec, amount: Dec) -> Self {
        self.book.asks.set_price_amount(price, amount);
        self
    }

    /// The price increment every level sits on. Panics unless positive.
    pub fn with_tick_size(mut self, tick_size: Dec) -> Self {
        if tick_size <= Dec::ZERO {
            panic!("Tick size must be positive");
        }
        self.tick_size = tick_size;
        self
    }

    /// The amount increment every level sits on. Panics unless positive.
    pub fn with_lot_size(mut self, lot_size: Dec) -> Self {
        if lot_size <= Dec::ZERO {
            panic!("Lot size must be positive");
        }
        self.lot_size = lot_size;
        self
    }

    /// The average spread a simulated step aims for. Panics unless positive.
    pub fn with_spread(mut self, spread: Dec) -> Self {
        if spread <= Dec::ZERO {
            panic!("Spread must be positive");
        }
        self.spread = spread;
        self
    }

    /// The average level amount. Panics unless positive.
    pub fn with_amount(mut self, amount: Dec) -> Self {
        if amount <= Dec::ZERO {
            panic!("Amount must be positive");
        }
        self.amount = amount;
        self
    }

    /// Arrival rate is used to simulate arrivals of cancel/update orders
    /// with distance from mid price.
    /// If the arrival rate is high, more orders will arrive closer to the mid price
    /// If the arrival is low, orders will be more uniformly distributed across the book.
    pub fn with_arrival_rate(mut self, arrival_rate: Dec) -> Self {
        if arrival_rate <= Dec::ZERO {
            panic!("Arrival rate must be positive");
        }
        self.arrival_rate = arrival_rate;
        self
    }

    /// The book as built, without simulating anything.
    pub fn build(self) -> OrderBook {
        self.book
    }

    /// Randomize levels around the given mid price,
    /// using the builder's tick size and spread
    ///
    /// Returns the simulated book and the events
    /// that would have led to it (trades, cancels, new orders)
    pub fn simulate(&mut self, mid: Dec, levels: usize) -> (OrderBook, OrderBookEvents) {
        let mut events = self.deplete();
        let half_spread = self.simulate_half_spread();
        let bid = (mid - half_spread).round_to_step(self.tick_size);
        let ask = (mid + half_spread).round_to_step(self.tick_size);
        events.trades = self.trades(bid, ask);
        events.new_orders = self.add_levels(bid, ask, levels);
        (self.book.clone(), events)
    }

    /// Returns a randomized spread with `self.spread` as the average, sampled
    /// uniformly in `[spread * 0.5, spread * 1.5]`.
    ///
    /// The jitter is drawn in percent and applied to the raw scale so the result
    /// stays an exact multiple of the tick size.
    pub fn simulate_spread(&mut self) -> Dec {
        let jitter: i128 = self.rng.random_range(50..150);
        ceil_to_step(scale_percent(self.spread, jitter), self.tick_size)
    }

    /// Half of [`OrderBookBuilder::simulate_spread`], rounded to the raw scale.
    pub fn simulate_half_spread(&mut self) -> Dec {
        Dec::from_raw(self.simulate_spread().into_raw() / 2)
    }

    /// A randomised level amount, at least one lot.
    pub fn simulate_amount(&mut self) -> Dec {
        let factor: i128 = self.rng.random_range(10..1_000);
        ceil_to_step(scale_percent(self.amount, factor), self.lot_size)
    }

    /// Returns a randomized distance from the best price using an exponential distribution
    /// with mean `self.spread / self.arrival_rate`.
    pub fn simulate_distance(&mut self) -> Dec {
        let rate = self.arrival_rate.to_f64() / self.spread.to_f64();
        let dist = Exp::new(rate).expect("rate must be positive");
        let distance = Dec::from_f64(dist.sample(&mut self.rng)).unwrap_or(self.spread);
        ceil_to_step(distance.max(self.tick_size), self.tick_size)
    }

    /// Simulate random cancellations of existing levels
    /// with higher probability for levels closer to the mid price.
    pub fn deplete(&mut self) -> OrderBookEvents {
        let mut events = OrderBookEvents::default();
        let Some(mid) = self.book.mid_price() else {
            return events;
        };
        let Some(spread) = self.book.spread() else {
            return events;
        };
        let lambda = self.arrival_rate.to_f64() / spread.to_f64();
        let depleted: Vec<Dec> = self
            .book
            .bids
            .iter()
            .filter_map(|level| {
                let distance = (mid - level.price).to_f64();
                // Deplete the level with higher probability the closer it is to the mid
                let depletion_chance = (-lambda * distance).exp();
                let roll: f64 = self.rng.random_range(0.0..1.0);
                if roll < depletion_chance {
                    events.cancels.push(PriceAmountSide {
                        price_amount: *level,
                        side: Side::Buy,
                    });
                    Some(level.price)
                } else {
                    None
                }
            })
            .collect();
        for price in depleted {
            self.book.bids.set_price_amount(price, Dec::ZERO);
        }
        // asks
        let depleted: Vec<Dec> = self
            .book
            .asks
            .iter()
            .filter_map(|level| {
                let distance = (level.price - mid).to_f64();
                // Deplete the level with higher probability the closer it is to the mid
                let depletion_chance = (-lambda * distance).exp();
                let roll: f64 = self.rng.random_range(0.0..1.0);
                if roll < depletion_chance {
                    events.cancels.push(PriceAmountSide {
                        price_amount: *level,
                        side: Side::Sell,
                    });
                    Some(level.price)
                } else {
                    None
                }
            })
            .collect();
        for price in depleted {
            self.book.asks.set_price_amount(price, Dec::ZERO);
        }
        events
    }

    fn trades(&mut self, bid: Dec, ask: Dec) -> Vec<PriceAmountSide> {
        let mut trades = Vec::new();
        loop {
            if let Some(level) = self.book.bids.best().copied()
                && level.price > bid
            {
                trades.push(PriceAmountSide {
                    price_amount: level,
                    side: Side::Sell,
                });
                self.book.bids.set_price_amount(level.price, Dec::ZERO);
            } else {
                break;
            }
        }
        loop {
            if let Some(level) = self.book.asks.best().copied()
                && level.price < ask
            {
                trades.push(PriceAmountSide {
                    price_amount: level,
                    side: Side::Buy,
                });
                self.book.asks.set_price_amount(level.price, Dec::ZERO);
            } else {
                break;
            }
        }
        trades
    }

    fn add_levels(&mut self, bid: Dec, ask: Dec, levels: usize) -> Vec<PriceAmountSide> {
        let mut new_orders = Vec::new();
        if self.book.bids.best_price() < Some(bid) {
            let amount = self.simulate_amount();
            self.book.bids.set_price_amount(bid, amount);
            self.book.bids.trim(levels);
        }
        if self.book.asks.best_price() > Some(ask) {
            let amount = self.simulate_amount();
            self.book.asks.set_price_amount(ask, amount);
            self.book.asks.trim(levels);
        }
        while self.book.bids.len() < levels {
            let price = bid - self.simulate_distance();
            let amount = self.simulate_amount();
            self.book.bids.set_price_amount(price, amount);
            new_orders.push(PriceAmountSide {
                price_amount: PriceAmount { price, amount },
                side: Side::Buy,
            });
        }
        while self.book.asks.len() < levels {
            let price = ask + self.simulate_distance();
            let amount = self.simulate_amount();
            self.book.asks.set_price_amount(price, amount);
            new_orders.push(PriceAmountSide {
                price_amount: PriceAmount { price, amount },
                side: Side::Sell,
            });
        }
        new_orders
    }
}

/// `value * percent / 100`, on the raw scale so no precision is lost
fn scale_percent(value: Dec, percent: i128) -> Dec {
    Dec::from_raw(value.into_raw() * percent / 100)
}

// always up, unlike [`Dec::round_to_step`], so a spread, amount or distance
// never rounds down to nothing
fn ceil_to_step(value: Dec, step: Dec) -> Dec {
    let (value, step) = (value.into_raw(), step.into_raw());
    let steps = value.div_euclid(step) + i128::from(value.rem_euclid(step) != 0);
    Dec::from_raw(steps * step)
}

/// A lognormal random walk, the usual stand-in for a price path.
#[derive(Debug)]
pub struct RandomWalk {
    steps: usize,
    rng: SmallRng,
}

impl RandomWalk {
    /// A walk of `steps` prices, seeded deterministically.
    pub fn new(steps: usize) -> Self {
        Self {
            steps,
            rng: SmallRng::seed_from_u64(0),
        }
    }

    /// Seed the generator, so a walk repeats exactly.
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.rng = SmallRng::seed_from_u64(seed);
        self
    }

    /// `steps` prices from `start`, each one the last multiplied by
    /// `exp((drift - volatility^2 / 2) dt + volatility sqrt(dt) Z)`.
    ///
    /// `drift` and `volatility` describe the whole path rather than one step,
    /// so `dt` is `1 / steps` and the shape of the path does not change with
    /// the number of steps taken through it. A negative volatility is the one
    /// input that fails.
    pub fn lognormal(
        mut self,
        start: f64,
        drift: f64,
        volatility: f64,
    ) -> Result<Vec<f64>, NormalError> {
        let dt = 1.0 / self.steps.max(1) as f64;
        let step = Normal::new(
            (drift - volatility * volatility / 2.0) * dt,
            volatility * dt.sqrt(),
        )?;
        let mut price = start;
        Ok((0..self.steps)
            .map(|_| {
                price *= step.sample(&mut self.rng).exp();
                price
            })
            .collect())
    }
}
