use crate::Dec;

/// A struct representing a price and amount pair.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PriceAmount {
    /// The price of the level.
    pub price: Dec,
    /// The amount of the level.
    pub amount: Dec,
}

impl PriceAmount {
    /// Whether this level can enter a book.
    ///
    /// A non-finite price would sort against every other level rather than
    /// among them, since [`Dec::NAN`] orders below [`Dec::MIN`]. A non-finite
    /// amount would carry into every statistic taken over the side for as long
    /// as the level stood. A negative amount is not a size.
    pub const fn is_valid(self) -> bool {
        self.price.is_finite() && self.amount.is_finite() && !self.amount.is_sign_negative()
    }
}
