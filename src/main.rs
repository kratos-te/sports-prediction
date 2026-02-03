use anyhow::Result;
use tracing::{info, error};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod config;
mod types;
mod data;
mod strategies;
mod execution;
mod risk;
mod monitoring;
mod models;

use config::Config;
use data::DataPipeline;
use execution::ExecutionEngine;
use risk::RiskManager;
use monitoring::MonitoringService;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "polymarket_trading_bot=debug,info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("🚀 Starting Polymarket Trading Bot v2.0");

    // Load configuration
    let config = Config::load()?;
    info!("✅ Configuration loaded");

    // Initialize database connection pool
    let db_pool = config.create_db_pool().await?;
    info!("✅ Database connected");

    // Initialize Redis connection
    let redis_client = config.create_redis_client().await?;
    info!("✅ Redis connected");

    // Initialize components
    let data_pipeline = DataPipeline::new(
        db_pool.clone(),
        redis_client.clone(),
        &config,
    ).await?;
    info!("✅ Data pipeline initialized");

    let risk_manager = RiskManager::new(db_pool.clone(), &config).await?;
    info!("✅ Risk manager initialized");

    let execution_engine = ExecutionEngine::new(
        db_pool.clone(),
        &config,
        risk_manager.clone(),
    ).await?;
    info!("✅ Execution engine initialized");

    let monitoring = MonitoringService::new(db_pool.clone(), &config)?;
    info!("✅ Monitoring service initialized");

    // Start all services
    let data_handle = tokio::spawn(async move {
        if let Err(e) = data_pipeline.run().await {
            error!("Data pipeline error: {}", e);
        }
    });

    let execution_handle = tokio::spawn(async move {
        if let Err(e) = execution_engine.run().await {
            error!("Execution engine error: {}", e);
        }
    });

    let monitoring_handle = tokio::spawn(async move {
        if let Err(e) = monitoring.run().await {
            error!("Monitoring service error: {}", e);
        }
    });

    info!("🎯 Trading bot is running...");
    info!("📊 Dashboard: http://localhost:3000");
    info!("📈 Metrics: http://localhost:9090/metrics");

    // Wait for all services
    tokio::select! {
        _ = data_handle => error!("Data pipeline stopped"),
        _ = execution_handle => error!("Execution engine stopped"),
        _ = monitoring_handle => error!("Monitoring service stopped"),
        _ = tokio::signal::ctrl_c() => {
            info!("🛑 Shutdown signal received");
        }
    }

    info!("👋 Polymarket Trading Bot stopped");
    Ok(())
}
