use anyhow::Result;
use serde::Deserialize;
use sqlx::{postgres::PgPoolOptions, PgPool};
use redis::Client as RedisClient;
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub blockchain: BlockchainConfig,
    pub polymarket: PolymarketConfig,
    pub strategies: StrategiesConfig,
    pub risk: RiskConfig,
    pub monitoring: MonitoringConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub connection_timeout: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RedisConfig {
    pub url: String,
    pub pool_size: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BlockchainConfig {
    pub polygon_rpc_url: String,
    pub polygon_ws_url: String,
    pub private_key: String,
    pub gas_limit: u64,
    pub max_gas_price_gwei: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PolymarketConfig {
    pub api_url: String,
    pub ws_url: String,
    pub api_key: Option<String>,
    pub ctf_exchange_address: String,
    pub conditional_tokens_address: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StrategiesConfig {
    pub clv_arb: ClvArbConfig,
    pub poisson_ev: PoissonEvConfig,
    pub news_scalp: NewsScalpConfig,
    pub enabled_strategies: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClvArbConfig {
    pub min_divergence_pct: f64,
    pub exit_on_convergence: bool,
    pub max_hold_hours: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PoissonEvConfig {
    pub min_edge_pct: f64,
    pub simulation_count: u32,
    pub min_significance: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewsScalpConfig {
    pub execution_timeout_seconds: u64,
    pub exit_after_minutes: u64,
    pub twitter_bearer_token: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RiskConfig {
    pub starting_capital: f64,
    pub max_position_size_pct: f64,
    pub daily_drawdown_limit_pct: f64,
    pub max_correlation: f64,
    pub min_market_liquidity: f64,
    pub max_daily_trades: i32,
    pub kelly_fraction: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MonitoringConfig {
    pub metrics_port: u16,
    pub dashboard_port: u16,
    pub telegram_bot_token: Option<String>,
    pub telegram_chat_id: Option<String>,
}

impl Config {
    /// Load configuration from file and environment variables
    pub fn load() -> Result<Self> {
        dotenv::dotenv().ok();
        
        let config = config::Config::builder()
            .add_source(config::File::with_name("config/default").required(false))
            .add_source(config::File::with_name("config/production").required(false))
            .add_source(config::Environment::with_prefix("TRADING_BOT"))
            .build()?;
        
        Ok(config.try_deserialize()?)
    }

    /// Create database connection pool
    pub async fn create_db_pool(&self) -> Result<PgPool> {
        let pool = PgPoolOptions::new()
            .max_connections(self.database.max_connections)
            .min_connections(self.database.min_connections)
            .acquire_timeout(Duration::from_secs(self.database.connection_timeout))
            .connect(&self.database.url)
            .await?;
        
        Ok(pool)
    }

    /// Create Redis client
    pub async fn create_redis_client(&self) -> Result<RedisClient> {
        let client = RedisClient::open(self.redis.url.clone())?;
        // Test connection
        let mut conn = client.get_connection()?;
        redis::cmd("PING").query::<String>(&mut conn)?;
        Ok(client)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            database: DatabaseConfig {
                url: "postgresql://localhost/polymarket_bot".to_string(),
                max_connections: 10,
                min_connections: 2,
                connection_timeout: 30,
            },
            redis: RedisConfig {
                url: "redis://localhost:6379".to_string(),
                pool_size: 10,
            },
            blockchain: BlockchainConfig {
                polygon_rpc_url: "https://polygon-rpc.com".to_string(),
                polygon_ws_url: "wss://polygon-rpc.com".to_string(),
                private_key: String::new(),
                gas_limit: 500000,
                max_gas_price_gwei: 100,
            },
            polymarket: PolymarketConfig {
                api_url: "https://api.polymarket.com".to_string(),
                ws_url: "wss://ws.polymarket.com".to_string(),
                api_key: None,
                ctf_exchange_address: "0x4bFb41d5B3570DeFd03C39a9A4D8dE6Bd8B8982E".to_string(),
                conditional_tokens_address: "0x4D97DCd97eC945f40cF65F87097ACe5EA0476045".to_string(),
            },
            strategies: StrategiesConfig {
                clv_arb: ClvArbConfig {
                    min_divergence_pct: 3.0,
                    exit_on_convergence: true,
                    max_hold_hours: 24,
                },
                poisson_ev: PoissonEvConfig {
                    min_edge_pct: 5.0,
                    simulation_count: 10000,
                    min_significance: 0.95,
                },
                news_scalp: NewsScalpConfig {
                    execution_timeout_seconds: 60,
                    exit_after_minutes: 15,
                    twitter_bearer_token: None,
                },
                enabled_strategies: vec![
                    "clv_arb".to_string(),
                    "poisson_ev".to_string(),
                ],
            },
            risk: RiskConfig {
                starting_capital: 50000.0,
                max_position_size_pct: 2.0,
                daily_drawdown_limit_pct: 8.0,
                max_correlation: 0.6,
                min_market_liquidity: 5000.0,
                max_daily_trades: 20,
                kelly_fraction: 0.5,
            },
            monitoring: MonitoringConfig {
                metrics_port: 9090,
                dashboard_port: 3000,
                telegram_bot_token: None,
                telegram_chat_id: None,
            },
        }
    }
}
