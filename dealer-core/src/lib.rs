mod convert;
mod deal;
mod fast_deal;
mod hand;
pub mod rng;
mod shape;
mod swap;

// Re-export core types from bridge-types
pub use bridge_types::{Card, Direction, Rank, Suit};

// Position is an alias for Direction for backwards compatibility
pub type Position = Direction;

pub use deal::Deal;
pub use fast_deal::{
    generate_deal_from_seed, generate_deal_from_seed_no_predeal, FastDealConfig, FastDealGenerator,
};
pub use hand::Hand;
pub use shape::{shape_to_index, ShapeMask};
pub use swap::SwapMode;
