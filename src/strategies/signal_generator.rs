use anyhow::Result;
use sqlx::PgPool;
use tokio::time::{interval, Duration};
use tracing::{info, error};

use crate::types::{Signal, Market};
use crate::config::Config;
use super::{Strategy, ClvArbitrageStrategy, PoissonEvStrategy};

pub struct SignalGenerator {
    db_pool: PgPool,
    strategies: Vec<Box<dyn Strategy>>,
}

impl SignalGenerator {
    pub async fn new(db_pool: PgPool, config: &Config) -> Result<Self> {
        let mut strategies: Vec<Box<dyn Strategy>> = Vec::new();

        // Initialize enabled strategies
        if config.strategies.enabled_strategies.contains(&"clv_arb".to_string()) {
            let clv_strategy = ClvArbitrageStrategy::new(
                db_pool.clone(),
                config.strategies.clv_arb.min_divergence_pct,
                config.strategies.clv_arb.max_hold_hours,
            );
            strategies.push(Box::new(clv_strategy));
            info!("✅ CLV Arbitrage strategy enabled");
        }

        if config.strategies.enabled_strategies.contains(&"poisson_ev".to_string()) {
            let poisson_strategy = PoissonEvStrategy::new(
                db_pool.clone(),
                config.strategies.poisson_ev.min_edge_pct,
                config.strategies.poisson_ev.simulation_count,
                config.strategies.poisson_ev.min_significance,
            );
            strategies.push(Box::new(poisson_strategy));
            info!("✅ Poisson EV strategy enabled");
        }

        Ok(Self {
            db_pool,
            strategies,
        })
    }

    pub async fn run(&self) -> Result<()> {
        let mut tick = interval(Duration::from_secs(60)); // Run every minute

        info!("🎯 Signal generator started with {} strategies", self.strategies.len());

        loop {
            tick.tick().await;

            if let Err(e) = self.generate_and_store_signals().await {
                error!("Error generating signals: {}", e);
            }
        }
    }

    async fn generate_and_store_signals(&self) -> Result<()> {
        // Fetch active markets
        let markets = self.fetch_active_markets().await?;
        
        if markets.is_empty() {
            return Ok(());
        }

        info!("📊 Analyzing {} markets", markets.len());

        // Run all strategies
        for strategy in &self.strategies {
            match strategy.generate_signals(&markets).await {
                Ok(signals) => {
                    if !signals.is_empty() {
                        info!("✨ {} generated {} signals", strategy.name(), signals.len());
                        self.store_signals(&signals).await?;
                    }
                }
                Err(e) => {
                    error!("Strategy {} error: {}", strategy.name(), e);
                }
            }
        }

        Ok(())
    }

    async fn fetch_active_markets(&self) -> Result<Vec<Market>> {
        let rows = sqlx::query!(
            r#"
            SELECT 
                market_id,
                sport,
                event_name,
                event_time,
                market_type,
                description,
                current_liquidity,
                yes_price,
                no_price,
                created_at,
                updated_at
            FROM markets
            WHERE status = 'active'
                AND event_time > NOW()
                AND current_liquidity >= 5000
            ORDER BY event_time ASC
            LIMIT 100
            "#
        )
        .fetch_all(&self.db_pool)
        .await?;

        let markets: Vec<Market> = rows.into_iter()
            .filter_map(|row| {
                // Parse the market data
                Some(Market {
                    market_id: row.market_id,
                    sport: match row.sport.as_str() {
                        "NFL" => crate::types::Sport::NFL,
                        "NBA" => crate::types::Sport::NBA,
                        "Premier League" => crate::types::Sport::PremierLeague,
                        "MLB" => crate::types::Sport::MLB,
                        _ => return None,
                    },
                    event_name: row.event_name,
                    event_time: row.event_time,
                    market_type: serde_json::from_str(&row.market_type).ok()?,
                    description: row.description,
                    resolution_source: None,
                    min_liquidity: rust_decimal::Decimal::ZERO,
                    current_liquidity: row.current_liquidity,
                    yes_price: row.yes_price,
                    no_price: row.no_price,
                    status: crate::types::MarketStatus::Active,
                    created_at: row.created_at,
                    updated_at: row.updated_at,
                })
            })
            .collect();

        Ok(markets)
    }

    async fn store_signals(&self, signals: &[Signal]) -> Result<()> {
        for signal in signals {
            sqlx::query!(
                r#"
                INSERT INTO signals (
                    signal_id, market_id, strategy, signal_type,
                    confidence, edge_size, recommended_size,
                    current_price, fair_value, metadata
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                "#,
                signal.signal_id,
                signal.market_id,
                signal.strategy.as_str(),
                serde_json::to_string(&signal.signal_type)?,
                signal.confidence,
                signal.edge_size,
                signal.recommended_size,
                signal.current_price,
                signal.fair_value,
                signal.metadata,
            )
            .execute(&self.db_pool)
            .await?;
        }

        Ok(())
    }
}
