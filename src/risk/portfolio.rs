use anyhow::Result;
use sqlx::PgPool;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use chrono::Utc;

use crate::types::PortfolioState;

pub struct PortfolioTracker {
    db_pool: PgPool,
    state: PortfolioState,
}

impl PortfolioTracker {
    pub async fn new(db_pool: PgPool, starting_capital: f64) -> Result<Self> {
        let starting_capital = Decimal::from_f64_retain(starting_capital)
            .unwrap_or(dec!(50000.0));

        let state = PortfolioState {
            total_capital: starting_capital,
            available_capital: starting_capital,
            invested_capital: dec!(0.0),
            unrealized_pnl: dec!(0.0),
            realized_pnl_today: dec!(0.0),
            daily_drawdown: dec!(0.0),
            max_drawdown: dec!(0.0),
            open_positions: 0,
            trades_today: 0,
            timestamp: Utc::now(),
        };

        // Store initial state
        let mut tracker = Self {
            db_pool,
            state,
        };
        
        tracker.refresh_state().await?;
        
        Ok(tracker)
    }

    /// Refresh portfolio state from database
    pub async fn refresh_state(&mut self) -> Result<()> {
        // Calculate current state from database
        let result = sqlx::query!(
            r#"
            SELECT * FROM calculate_portfolio_state()
            "#
        )
        .fetch_one(&self.db_pool)
        .await?;

        let total_capital = result.total_capital.unwrap_or(dec!(50000.0));
        let available_capital = result.available_capital.unwrap_or(total_capital);
        let invested_capital = result.invested_capital.unwrap_or(dec!(0.0));
        let unrealized_pnl = result.unrealized_pnl.unwrap_or(dec!(0.0));

        // Get today's realized PnL
        let today_pnl = sqlx::query!(
            r#"
            SELECT COALESCE(SUM(pnl), 0) as total_pnl
            FROM trades
            WHERE status = 'closed'
                AND DATE(exit_time) = CURRENT_DATE
            "#
        )
        .fetch_one(&self.db_pool)
        .await?;

        let realized_pnl_today = today_pnl.total_pnl.unwrap_or(dec!(0.0));

        // Get open positions count
        let positions = sqlx::query!(
            r#"
            SELECT COUNT(*) as count
            FROM trades
            WHERE status = 'open'
            "#
        )
        .fetch_one(&self.db_pool)
        .await?;

        let open_positions = positions.count.unwrap_or(0) as i32;

        // Get trades today count
        let trades = sqlx::query!(
            r#"
            SELECT COUNT(*) as count
            FROM trades
            WHERE DATE(entry_time) = CURRENT_DATE
            "#
        )
        .fetch_one(&self.db_pool)
        .await?;

        let trades_today = trades.count.unwrap_or(0) as i32;

        // Calculate daily drawdown
        let daily_drawdown = if total_capital > dec!(0.0) {
            (realized_pnl_today / total_capital) * dec!(-100.0)
        } else {
            dec!(0.0)
        }.max(dec!(0.0));

        self.state = PortfolioState {
            total_capital,
            available_capital,
            invested_capital,
            unrealized_pnl,
            realized_pnl_today,
            daily_drawdown,
            max_drawdown: self.state.max_drawdown.max(daily_drawdown),
            open_positions,
            trades_today,
            timestamp: Utc::now(),
        };

        // Store snapshot
        self.store_snapshot().await?;

        Ok(())
    }

    /// Update PnL after a trade
    pub async fn update_pnl(&mut self, pnl: Decimal) -> Result<()> {
        self.state.realized_pnl_today += pnl;
        self.state.total_capital += pnl;
        self.refresh_state().await?;
        Ok(())
    }

    /// Get current portfolio state
    pub fn get_state(&self) -> &PortfolioState {
        &self.state
    }

    /// Store portfolio snapshot to database
    async fn store_snapshot(&self) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO portfolio_state (
                total_capital, available_capital, invested_capital,
                unrealized_pnl, realized_pnl_today, daily_drawdown,
                max_drawdown, open_positions, trades_today
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
            self.state.total_capital,
            self.state.available_capital,
            self.state.invested_capital,
            self.state.unrealized_pnl,
            self.state.realized_pnl_today,
            self.state.daily_drawdown,
            self.state.max_drawdown,
            self.state.open_positions,
            self.state.trades_today,
        )
        .execute(&self.db_pool)
        .await?;

        Ok(())
    }
}
