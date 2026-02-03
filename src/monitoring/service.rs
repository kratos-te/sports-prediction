use anyhow::Result;
use sqlx::PgPool;
use tokio::time::{interval, Duration};
use tracing::{info, error};

use crate::config::Config;
use super::MetricsCollector;

pub struct MonitoringService {
    db_pool: PgPool,
    metrics_collector: MetricsCollector,
}

impl MonitoringService {
    pub fn new(db_pool: PgPool, config: &Config) -> Result<Self> {
        let metrics_collector = MetricsCollector::new(config)?;

        Ok(Self {
            db_pool,
            metrics_collector,
        })
    }

    pub async fn run(&self) -> Result<()> {
        let mut tick = interval(Duration::from_secs(60)); // Update every minute

        info!("📊 Monitoring service started");

        loop {
            tick.tick().await;

            if let Err(e) = self.collect_metrics().await {
                error!("Error collecting metrics: {}", e);
            }

            if let Err(e) = self.update_performance_metrics().await {
                error!("Error updating performance: {}", e);
            }
        }
    }

    async fn collect_metrics(&self) -> Result<()> {
        // Collect portfolio metrics
        let portfolio = sqlx::query!(
            r#"
            SELECT * FROM v_portfolio_summary
            LIMIT 1
            "#
        )
        .fetch_optional(&self.db_pool)
        .await?;

        if let Some(p) = portfolio {
            self.metrics_collector.record_portfolio_value(
                p.total_capital.unwrap_or_default()
            );
            self.metrics_collector.record_open_positions(
                p.open_positions.unwrap_or(0) as i64
            );
        }

        // Collect trade metrics
        let trades = sqlx::query!(
            r#"
            SELECT COUNT(*) as count
            FROM trades
            WHERE DATE(entry_time) = CURRENT_DATE
            "#
        )
        .fetch_one(&self.db_pool)
        .await?;

        self.metrics_collector.record_daily_trades(
            trades.count.unwrap_or(0)
        );

        Ok(())
    }

    async fn update_performance_metrics(&self) -> Result<()> {
        // Calculate and store daily performance metrics
        // This would update the performance table with Sharpe ratio, etc.
        
        Ok(())
    }
}
