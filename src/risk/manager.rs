use anyhow::{Result, bail};
use sqlx::PgPool;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::config::Config;
use crate::types::{Signal, RiskLimits, PortfolioState};
use super::PortfolioTracker;

#[derive(Clone)]
pub struct RiskManager {
    db_pool: PgPool,
    limits: RiskLimits,
    portfolio_tracker: Arc<RwLock<PortfolioTracker>>,
}

impl RiskManager {
    pub async fn new(db_pool: PgPool, config: &Config) -> Result<Self> {
        let limits = RiskLimits {
            max_position_size_pct: Decimal::from_f64_retain(config.risk.max_position_size_pct)
                .unwrap_or(dec!(2.0)),
            daily_drawdown_limit_pct: Decimal::from_f64_retain(config.risk.daily_drawdown_limit_pct)
                .unwrap_or(dec!(8.0)),
            max_correlation: Decimal::from_f64_retain(config.risk.max_correlation)
                .unwrap_or(dec!(0.6)),
            min_market_liquidity: Decimal::from_f64_retain(config.risk.min_market_liquidity)
                .unwrap_or(dec!(5000.0)),
            max_daily_trades: config.risk.max_daily_trades,
            cooldown_after_losses: 3,
            cooldown_period_minutes: 60,
            kelly_fraction: Decimal::from_f64_retain(config.risk.kelly_fraction)
                .unwrap_or(dec!(0.5)),
            min_edge_size: dec!(0.03),
        };

        let portfolio_tracker = Arc::new(RwLock::new(
            PortfolioTracker::new(db_pool.clone(), config.risk.starting_capital).await?
        ));

        Ok(Self {
            db_pool,
            limits,
            portfolio_tracker,
        })
    }

    /// Validate if a signal passes all risk checks
    pub async fn validate_signal(&self, signal: &Signal) -> Result<bool> {
        // Check if circuit breaker is active
        if self.is_circuit_breaker_active().await? {
            warn!("⚠️ Circuit breaker active - rejecting signal");
            return Ok(false);
        }

        // Check edge size
        if signal.edge_size < self.limits.min_edge_size {
            return Ok(false);
        }

        // Check daily trade limit
        let portfolio = self.portfolio_tracker.read().await;
        if portfolio.get_state().trades_today >= self.limits.max_daily_trades {
            warn!("⚠️ Daily trade limit reached");
            return Ok(false);
        }

        // Check daily drawdown
        let state = portfolio.get_state();
        if state.daily_drawdown >= self.limits.daily_drawdown_limit_pct {
            warn!("⚠️ Daily drawdown limit reached: {:.2}%", state.daily_drawdown);
            return Ok(false);
        }

        Ok(true)
    }

    /// Calculate optimal position size using Kelly Criterion with risk limits
    pub async fn calculate_position_size(&self, signal: &Signal) -> Result<Decimal> {
        let portfolio = self.portfolio_tracker.read().await;
        let state = portfolio.get_state();

        // Calculate position size using Kelly Criterion
        let win_probability = signal.fair_value;
        let edge = signal.edge_size;

        let position_size = state.calculate_position_size(
            edge,
            win_probability,
            self.limits.kelly_fraction,
            self.limits.max_position_size_pct,
        );

        // Ensure we have enough available capital
        let max_available = state.available_capital * dec!(0.95); // Keep 5% buffer
        let final_size = position_size.min(max_available);

        info!(
            "💰 Position sizing: Kelly={:.2}, Max={:.2}, Final={:.2}",
            position_size, max_available, final_size
        );

        Ok(final_size)
    }

    /// Check if any circuit breakers are active
    async fn is_circuit_breaker_active(&self) -> Result<bool> {
        let result = sqlx::query!(
            r#"
            SELECT COUNT(*) as count
            FROM circuit_breakers
            WHERE status = 'active'
            "#
        )
        .fetch_one(&self.db_pool)
        .await?;

        Ok(result.count.unwrap_or(0) > 0)
    }

    /// Trigger circuit breaker
    pub async fn trigger_circuit_breaker(&self, reason: String) -> Result<()> {
        warn!("🚨 CIRCUIT BREAKER TRIGGERED: {}", reason);

        sqlx::query!(
            r#"
            INSERT INTO circuit_breakers (reason, metadata)
            VALUES ($1, $2)
            "#,
            reason,
            serde_json::json!({
                "timestamp": chrono::Utc::now(),
            })
        )
        .execute(&self.db_pool)
        .await?;

        Ok(())
    }

    /// Check for consecutive losses and trigger cooldown if needed
    pub async fn check_consecutive_losses(&self) -> Result<()> {
        let recent_trades = sqlx::query!(
            r#"
            SELECT pnl
            FROM trades
            WHERE status = 'closed'
                AND entry_time > NOW() - INTERVAL '1 hour'
            ORDER BY entry_time DESC
            LIMIT 5
            "#
        )
        .fetch_all(&self.db_pool)
        .await?;

        let mut consecutive_losses = 0;
        for trade in recent_trades {
            if let Some(pnl) = trade.pnl {
                if pnl < dec!(0.0) {
                    consecutive_losses += 1;
                } else {
                    break;
                }
            }
        }

        if consecutive_losses >= self.limits.cooldown_after_losses {
            self.trigger_circuit_breaker(format!(
                "{} consecutive losses - cooldown activated",
                consecutive_losses
            )).await?;
        }

        Ok(())
    }

    /// Update portfolio state after a trade
    pub async fn update_portfolio(&self, trade_pnl: Decimal) -> Result<()> {
        let mut portfolio = self.portfolio_tracker.write().await;
        portfolio.update_pnl(trade_pnl).await?;
        
        // Check if we hit daily drawdown limit
        let state = portfolio.get_state();
        if state.daily_drawdown >= self.limits.daily_drawdown_limit_pct {
            self.trigger_circuit_breaker(format!(
                "Daily drawdown limit reached: {:.2}%",
                state.daily_drawdown
            )).await?;
        }

        Ok(())
    }

    /// Get current portfolio state
    pub async fn get_portfolio_state(&self) -> PortfolioState {
        let portfolio = self.portfolio_tracker.read().await;
        portfolio.get_state().clone()
    }
}
