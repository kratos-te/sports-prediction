mod clv_arbitrage;
mod poisson_ev;
mod signal_generator;

pub use clv_arbitrage::ClvArbitrageStrategy;
pub use poisson_ev::PoissonEvStrategy;
pub use signal_generator::SignalGenerator;

use async_trait::async_trait;
use anyhow::Result;
use crate::types::{Market, Signal};

/// Trait that all trading strategies must implement
#[async_trait]
pub trait Strategy: Send + Sync {
    /// Generate signals for given markets
    async fn generate_signals(&self, markets: &[Market]) -> Result<Vec<Signal>>;
    
    /// Get strategy name
    fn name(&self) -> &str;
    
    /// Check if strategy is enabled
    fn is_enabled(&self) -> bool;
}
