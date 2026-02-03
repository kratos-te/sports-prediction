use anyhow::Result;
use sqlx::PgPool;
use tokio::time::{interval, Duration};
use tracing::{info, warn, error};
use uuid::Uuid;
use rust_decimal::Decimal;
use chrono::Utc;

use crate::config::Config;
use crate::types::{Signal, Trade, TradeStatus, Position};
use crate::risk::RiskManager;
use super::BlockchainClient;

pub struct ExecutionEngine {
    db_pool: PgPool,
    blockchain_client: BlockchainClient,
    risk_manager: RiskManager,
}

impl ExecutionEngine {
    pub async fn new(
        db_pool: PgPool,
        config: &Config,
        risk_manager: RiskManager,
    ) -> Result<Self> {
        let blockchain_client = BlockchainClient::new(config)?;

        Ok(Self {
            db_pool,
            blockchain_client,
            risk_manager,
        })
    }

    pub async fn run(&self) -> Result<()> {
        let mut tick = interval(Duration::from_secs(10)); // Check every 10 seconds

        info!("⚡ Execution engine started");

        loop {
            tick.tick().await;

            // Process pending signals
            if let Err(e) = self.process_pending_signals().await {
                error!("Error processing signals: {}", e);
            }

            // Monitor open positions
            if let Err(e) = self.monitor_positions().await {
                error!("Error monitoring positions: {}", e);
            }
        }
    }

    async fn process_pending_signals(&self) -> Result<()> {
        // Fetch unexecuted signals
        let signals = self.fetch_pending_signals().await?;

        for signal in signals {
            if let Err(e) = self.execute_signal(&signal).await {
                error!("Failed to execute signal {}: {}", signal.signal_id, e);
            }
        }

        Ok(())
    }

    async fn fetch_pending_signals(&self) -> Result<Vec<Signal>> {
        let rows = sqlx::query!(
            r#"
            SELECT 
                signal_id, market_id, strategy, signal_type,
                confidence, edge_size, recommended_size,
                current_price, fair_value, generated_at, metadata
            FROM signals
            WHERE executed = FALSE
                AND generated_at > NOW() - INTERVAL '5 minutes'
            ORDER BY confidence DESC, edge_size DESC
            LIMIT 10
            "#
        )
        .fetch_all(&self.db_pool)
        .await?;

        let signals: Vec<Signal> = rows.into_iter()
            .filter_map(|row| {
                Some(Signal {
                    signal_id: row.signal_id,
                    market_id: row.market_id,
                    strategy: match row.strategy.as_str() {
                        "clv_arb" => crate::types::Strategy::ClvArbitrage,
                        "poisson_ev" => crate::types::Strategy::PoissonExpectedValue,
                        _ => return None,
                    },
                    signal_type: serde_json::from_str(&row.signal_type).ok()?,
                    confidence: row.confidence,
                    edge_size: row.edge_size,
                    recommended_size: row.recommended_size,
                    current_price: row.current_price,
                    fair_value: row.fair_value,
                    generated_at: row.generated_at,
                    metadata: row.metadata,
                })
            })
            .collect();

        Ok(signals)
    }

    async fn execute_signal(&self, signal: &Signal) -> Result<()> {
        info!("⚡ Executing signal {} for market {}", signal.signal_id, signal.market_id);

        // Validate signal through risk management
        if !self.risk_manager.validate_signal(signal).await? {
            warn!("Signal {} failed risk validation", signal.signal_id);
            self.mark_signal_executed(signal.signal_id, None).await?;
            return Ok(());
        }

        // Calculate position size
        let position_size = self.risk_manager.calculate_position_size(signal).await?;

        if position_size <= Decimal::ZERO {
            warn!("Position size is zero or negative for signal {}", signal.signal_id);
            self.mark_signal_executed(signal.signal_id, None).await?;
            return Ok(());
        }

        // Execute trade on blockchain
        let position = signal.signal_type.to_position();
        match self.blockchain_client.execute_trade(
            &signal.market_id,
            position,
            position_size,
            signal.current_price,
        ).await {
            Ok(tx_hash) => {
                info!("✅ Trade executed: {}", tx_hash);

                // Record trade in database
                let trade_id = self.record_trade(signal, position_size, tx_hash).await?;

                // Mark signal as executed
                self.mark_signal_executed(signal.signal_id, Some(trade_id)).await?;

                info!("💼 Trade {} recorded for signal {}", trade_id, signal.signal_id);
            }
            Err(e) => {
                error!("❌ Trade execution failed: {}", e);
                // Mark signal as executed to avoid retry (with failure noted)
                self.mark_signal_executed(signal.signal_id, None).await?;
            }
        }

        Ok(())
    }

    async fn record_trade(
        &self,
        signal: &Signal,
        quantity: Decimal,
        tx_hash: String,
    ) -> Result<Uuid> {
        let trade_id = Uuid::new_v4();
        let position = signal.signal_type.to_position();

        sqlx::query!(
            r#"
            INSERT INTO trades (
                trade_id, market_id, strategy, position, quantity,
                entry_price, entry_time, tx_hash_entry, status
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
            trade_id,
            signal.market_id,
            signal.strategy.as_str(),
            position.as_str(),
            quantity,
            signal.current_price,
            Utc::now(),
            tx_hash,
            "open",
        )
        .execute(&self.db_pool)
        .await?;

        Ok(trade_id)
    }

    async fn mark_signal_executed(&self, signal_id: Uuid, trade_id: Option<Uuid>) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE signals
            SET executed = TRUE, executed_trade_id = $2
            WHERE signal_id = $1
            "#,
            signal_id,
            trade_id,
        )
        .execute(&self.db_pool)
        .await?;

        Ok(())
    }

    async fn monitor_positions(&self) -> Result<()> {
        // Fetch open positions
        let positions = self.fetch_open_positions().await?;

        for trade in positions {
            // Check for exit conditions
            if self.should_exit_position(&trade).await? {
                if let Err(e) = self.close_position(&trade).await {
                    error!("Failed to close position {}: {}", trade.trade_id, e);
                }
            }
        }

        Ok(())
    }

    async fn fetch_open_positions(&self) -> Result<Vec<Trade>> {
        let rows = sqlx::query!(
            r#"
            SELECT 
                trade_id, market_id, strategy, position, quantity,
                entry_price, entry_time, tx_hash_entry, gas_cost
            FROM trades
            WHERE status = 'open'
            "#
        )
        .fetch_all(&self.db_pool)
        .await?;

        let trades: Vec<Trade> = rows.into_iter()
            .filter_map(|row| {
                Some(Trade {
                    trade_id: row.trade_id,
                    market_id: row.market_id,
                    strategy: match row.strategy.as_str() {
                        "clv_arb" => crate::types::Strategy::ClvArbitrage,
                        "poisson_ev" => crate::types::Strategy::PoissonExpectedValue,
                        _ => return None,
                    },
                    position: match row.position.as_str() {
                        "yes" => Position::Yes,
                        "no" => Position::No,
                        _ => return None,
                    },
                    quantity: row.quantity,
                    entry_price: row.entry_price,
                    exit_price: None,
                    entry_time: row.entry_time,
                    exit_time: None,
                    gas_cost: row.gas_cost,
                    slippage: None,
                    pnl: None,
                    pnl_percent: None,
                    status: TradeStatus::Open,
                    tx_hash_entry: row.tx_hash_entry,
                    tx_hash_exit: None,
                })
            })
            .collect();

        Ok(trades)
    }

    async fn should_exit_position(&self, _trade: &Trade) -> Result<bool> {
        // Implement exit logic:
        // 1. Check if market is resolved
        // 2. Check stop-loss conditions
        // 3. Check take-profit conditions
        // 4. Check time-based exits
        
        // For now, return false (hold until resolution)
        Ok(false)
    }

    async fn close_position(&self, trade: &Trade) -> Result<()> {
        info!("🔻 Closing position {}", trade.trade_id);

        // Execute exit trade on blockchain
        let opposite_position = match trade.position {
            Position::Yes => Position::No,
            Position::No => Position::Yes,
        };

        // Get current market price
        let current_price = self.get_current_price(&trade.market_id, trade.position).await?;

        match self.blockchain_client.execute_trade(
            &trade.market_id,
            opposite_position,
            trade.quantity,
            current_price,
        ).await {
            Ok(tx_hash) => {
                let pnl = (current_price - trade.entry_price) * trade.quantity;
                
                // Update trade in database
                sqlx::query!(
                    r#"
                    UPDATE trades
                    SET exit_price = $2,
                        exit_time = $3,
                        pnl = $4,
                        status = 'closed',
                        tx_hash_exit = $5
                    WHERE trade_id = $1
                    "#,
                    trade.trade_id,
                    current_price,
                    Utc::now(),
                    pnl,
                    tx_hash,
                )
                .execute(&self.db_pool)
                .await?;

                // Update portfolio
                self.risk_manager.update_portfolio(pnl).await?;

                info!("✅ Position closed with PnL: {}", pnl);
            }
            Err(e) => {
                error!("Failed to close position: {}", e);
            }
        }

        Ok(())
    }

    async fn get_current_price(&self, market_id: &str, position: Position) -> Result<Decimal> {
        let market = sqlx::query!(
            r#"
            SELECT yes_price, no_price
            FROM markets
            WHERE market_id = $1
            "#,
            market_id
        )
        .fetch_one(&self.db_pool)
        .await?;

        Ok(match position {
            Position::Yes => market.yes_price,
            Position::No => market.no_price,
        })
    }
}
