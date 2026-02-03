use anyhow::Result;
use prometheus::{Registry, Gauge, Counter, IntGauge};
use lazy_static::lazy_static;
use rust_decimal::Decimal;

use crate::config::Config;

lazy_static! {
    static ref REGISTRY: Registry = Registry::new();
    
    static ref PORTFOLIO_VALUE: Gauge = Gauge::new(
        "portfolio_total_value",
        "Total portfolio value in USD"
    ).unwrap();
    
    static ref OPEN_POSITIONS: IntGauge = IntGauge::new(
        "open_positions_count",
        "Number of open positions"
    ).unwrap();
    
    static ref DAILY_TRADES: Counter = Counter::new(
        "daily_trades_total",
        "Total trades executed today"
    ).unwrap();
    
    static ref SIGNALS_GENERATED: Counter = Counter::new(
        "signals_generated_total",
        "Total signals generated"
    ).unwrap();
}

pub struct MetricsCollector {
    _registry: &'static Registry,
}

impl MetricsCollector {
    pub fn new(_config: &Config) -> Result<Self> {
        // Register metrics
        REGISTRY.register(Box::new(PORTFOLIO_VALUE.clone()))?;
        REGISTRY.register(Box::new(OPEN_POSITIONS.clone()))?;
        REGISTRY.register(Box::new(DAILY_TRADES.clone()))?;
        REGISTRY.register(Box::new(SIGNALS_GENERATED.clone()))?;

        Ok(Self {
            _registry: &REGISTRY,
        })
    }

    pub fn record_portfolio_value(&self, value: Decimal) {
        if let Some(value_f64) = value.to_f64() {
            PORTFOLIO_VALUE.set(value_f64);
        }
    }

    pub fn record_open_positions(&self, count: i64) {
        OPEN_POSITIONS.set(count);
    }

    pub fn record_daily_trades(&self, count: i64) {
        DAILY_TRADES.reset();
        for _ in 0..count {
            DAILY_TRADES.inc();
        }
    }

    pub fn record_signal_generated(&self) {
        SIGNALS_GENERATED.inc();
    }
}
