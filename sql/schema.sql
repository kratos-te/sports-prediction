-- Polymarket Trading Bot Database Schema
-- Database: PostgreSQL 15+ with TimescaleDB extension

-- Enable extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS timescaledb;

-- ============================================================================
-- CORE TABLES
-- ============================================================================

-- Markets table: stores all Polymarket sports prediction markets
CREATE TABLE markets (
    market_id VARCHAR(66) PRIMARY KEY,  -- Polymarket market address
    sport VARCHAR(50) NOT NULL,         -- NFL, NBA, Premier League, MLB
    event_name TEXT NOT NULL,
    event_time TIMESTAMPTZ NOT NULL,
    market_type VARCHAR(50) NOT NULL,   -- moneyline, spread, total, prop
    description TEXT,
    resolution_source VARCHAR(100),
    min_liquidity DECIMAL(20, 2),
    current_liquidity DECIMAL(20, 2),
    yes_price DECIMAL(10, 8),
    no_price DECIMAL(10, 8),
    status VARCHAR(20) DEFAULT 'active', -- active, closed, resolved
    resolution VARCHAR(10),               -- yes, no, invalid
    resolved_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_markets_sport ON markets(sport);
CREATE INDEX idx_markets_event_time ON markets(event_time);
CREATE INDEX idx_markets_status ON markets(status);

-- Trades table: all executed trades with full details
CREATE TABLE trades (
    trade_id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    market_id VARCHAR(66) NOT NULL REFERENCES markets(market_id),
    strategy VARCHAR(50) NOT NULL,      -- clv_arb, poisson_ev, news_scalp, etc.
    position VARCHAR(10) NOT NULL,      -- yes, no
    quantity DECIMAL(20, 8) NOT NULL,
    entry_price DECIMAL(10, 8) NOT NULL,
    exit_price DECIMAL(10, 8),
    entry_time TIMESTAMPTZ DEFAULT NOW(),
    exit_time TIMESTAMPTZ,
    gas_cost DECIMAL(10, 4),
    slippage DECIMAL(10, 8),
    pnl DECIMAL(20, 4),
    pnl_percent DECIMAL(10, 4),
    status VARCHAR(20) DEFAULT 'open',  -- open, closed, stopped_out
    tx_hash_entry VARCHAR(66),
    tx_hash_exit VARCHAR(66),
    notes TEXT
);

CREATE INDEX idx_trades_market_id ON trades(market_id);
CREATE INDEX idx_trades_strategy ON trades(strategy);
CREATE INDEX idx_trades_entry_time ON trades(entry_time);
CREATE INDEX idx_trades_status ON trades(status);

-- Convert trades to hypertable for time-series optimization
SELECT create_hypertable('trades', 'entry_time', if_not_exists => TRUE);

-- Signals table: all generated signals before execution
CREATE TABLE signals (
    signal_id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    market_id VARCHAR(66) NOT NULL REFERENCES markets(market_id),
    strategy VARCHAR(50) NOT NULL,
    signal_type VARCHAR(10) NOT NULL,   -- buy_yes, buy_no
    confidence DECIMAL(5, 4) NOT NULL,  -- 0.0 to 1.0
    edge_size DECIMAL(10, 6) NOT NULL,  -- probability edge in decimal
    recommended_size DECIMAL(20, 8),
    current_price DECIMAL(10, 8),
    fair_value DECIMAL(10, 8),
    executed BOOLEAN DEFAULT FALSE,
    executed_trade_id UUID REFERENCES trades(trade_id),
    generated_at TIMESTAMPTZ DEFAULT NOW(),
    metadata JSONB                      -- strategy-specific data
);

CREATE INDEX idx_signals_market_id ON signals(market_id);
CREATE INDEX idx_signals_strategy ON signals(strategy);
CREATE INDEX idx_signals_generated_at ON signals(generated_at);
CREATE INDEX idx_signals_executed ON signals(executed);

SELECT create_hypertable('signals', 'generated_at', if_not_exists => TRUE);

-- Performance table: daily performance metrics by strategy
CREATE TABLE performance (
    performance_id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    date DATE NOT NULL,
    strategy VARCHAR(50) NOT NULL,
    trades_count INTEGER DEFAULT 0,
    wins INTEGER DEFAULT 0,
    losses INTEGER DEFAULT 0,
    total_pnl DECIMAL(20, 4) DEFAULT 0,
    gross_pnl DECIMAL(20, 4) DEFAULT 0,
    gas_costs DECIMAL(10, 4) DEFAULT 0,
    slippage_costs DECIMAL(10, 4) DEFAULT 0,
    sharpe_ratio DECIMAL(10, 4),
    sortino_ratio DECIMAL(10, 4),
    max_drawdown DECIMAL(10, 4),
    win_rate DECIMAL(5, 4),
    profit_factor DECIMAL(10, 4),
    avg_win DECIMAL(20, 4),
    avg_loss DECIMAL(20, 4),
    largest_win DECIMAL(20, 4),
    largest_loss DECIMAL(20, 4),
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE UNIQUE INDEX idx_performance_date_strategy ON performance(date, strategy);
CREATE INDEX idx_performance_date ON performance(date);

-- Bookmaker odds: reference odds from sharp bookmakers
CREATE TABLE bookmaker_odds (
    odds_id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    market_id VARCHAR(66) NOT NULL REFERENCES markets(market_id),
    bookmaker VARCHAR(50) NOT NULL,     -- pinnacle, betfair, etc.
    odds_type VARCHAR(20) NOT NULL,     -- moneyline, spread, total
    line_value DECIMAL(10, 2),          -- point spread or total value
    yes_odds DECIMAL(10, 4),            -- decimal odds
    no_odds DECIMAL(10, 4),
    yes_implied_prob DECIMAL(10, 8),
    no_implied_prob DECIMAL(10, 8),
    timestamp TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_bookmaker_odds_market_id ON bookmaker_odds(market_id);
CREATE INDEX idx_bookmaker_odds_timestamp ON bookmaker_odds(timestamp);

SELECT create_hypertable('bookmaker_odds', 'timestamp', if_not_exists => TRUE);

-- Whale wallets: tracked informed trader addresses
CREATE TABLE whale_wallets (
    wallet_address VARCHAR(42) PRIMARY KEY,
    label VARCHAR(100),
    total_volume DECIMAL(20, 2) DEFAULT 0,
    total_trades INTEGER DEFAULT 0,
    win_rate DECIMAL(5, 4),
    roi DECIMAL(10, 4),
    is_informed BOOLEAN DEFAULT TRUE,
    tracked_since TIMESTAMPTZ DEFAULT NOW(),
    last_activity TIMESTAMPTZ,
    notes TEXT
);

-- Whale trades: tracking whale wallet activity
CREATE TABLE whale_trades (
    whale_trade_id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    wallet_address VARCHAR(42) NOT NULL REFERENCES whale_wallets(wallet_address),
    market_id VARCHAR(66) NOT NULL REFERENCES markets(market_id),
    position VARCHAR(10) NOT NULL,
    quantity DECIMAL(20, 8) NOT NULL,
    price DECIMAL(10, 8) NOT NULL,
    tx_hash VARCHAR(66),
    block_number BIGINT,
    timestamp TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_whale_trades_wallet ON whale_trades(wallet_address);
CREATE INDEX idx_whale_trades_market ON whale_trades(market_id);
CREATE INDEX idx_whale_trades_timestamp ON whale_trades(timestamp);

SELECT create_hypertable('whale_trades', 'timestamp', if_not_exists => TRUE);

-- ============================================================================
-- RISK MANAGEMENT TABLES
-- ============================================================================

-- Portfolio state: current portfolio status
CREATE TABLE portfolio_state (
    snapshot_id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    total_capital DECIMAL(20, 4) NOT NULL,
    available_capital DECIMAL(20, 4) NOT NULL,
    invested_capital DECIMAL(20, 4) NOT NULL,
    unrealized_pnl DECIMAL(20, 4) DEFAULT 0,
    realized_pnl_today DECIMAL(20, 4) DEFAULT 0,
    daily_drawdown DECIMAL(10, 4) DEFAULT 0,
    max_drawdown DECIMAL(10, 4) DEFAULT 0,
    open_positions INTEGER DEFAULT 0,
    trades_today INTEGER DEFAULT 0,
    timestamp TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_portfolio_state_timestamp ON portfolio_state(timestamp);
SELECT create_hypertable('portfolio_state', 'timestamp', if_not_exists => TRUE);

-- Risk limits: configurable risk parameters
CREATE TABLE risk_limits (
    limit_id SERIAL PRIMARY KEY,
    parameter VARCHAR(50) UNIQUE NOT NULL,
    value DECIMAL(20, 8) NOT NULL,
    description TEXT,
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Insert default risk limits
INSERT INTO risk_limits (parameter, value, description) VALUES
('max_position_size_pct', 2.0, 'Maximum position size as % of portfolio'),
('daily_drawdown_limit_pct', 8.0, 'Maximum daily drawdown before circuit breaker'),
('max_correlation', 0.6, 'Maximum correlation between positions'),
('min_market_liquidity', 5000.0, 'Minimum market liquidity in USD'),
('max_daily_trades', 20, 'Maximum trades per day'),
('cooldown_after_losses', 3, 'Number of consecutive losses before cooldown'),
('cooldown_period_minutes', 60, 'Cooldown period in minutes'),
('kelly_fraction', 0.5, 'Kelly criterion fraction (0.5 = half-Kelly)'),
('min_edge_size', 0.03, 'Minimum edge size to take trade (3%)');

-- Circuit breakers: active circuit breaker events
CREATE TABLE circuit_breakers (
    breaker_id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    reason VARCHAR(100) NOT NULL,
    triggered_at TIMESTAMPTZ DEFAULT NOW(),
    cleared_at TIMESTAMPTZ,
    status VARCHAR(20) DEFAULT 'active', -- active, cleared
    metadata JSONB
);

CREATE INDEX idx_circuit_breakers_status ON circuit_breakers(status);

-- ============================================================================
-- ANALYTICS & MONITORING TABLES
-- ============================================================================

-- System logs: important system events
CREATE TABLE system_logs (
    log_id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    level VARCHAR(20) NOT NULL,         -- INFO, WARN, ERROR, CRITICAL
    component VARCHAR(50) NOT NULL,     -- data_ingestion, execution, risk_mgmt
    message TEXT NOT NULL,
    metadata JSONB,
    timestamp TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_system_logs_level ON system_logs(level);
CREATE INDEX idx_system_logs_timestamp ON system_logs(timestamp);
SELECT create_hypertable('system_logs', 'timestamp', if_not_exists => TRUE);

-- API requests: tracking external API calls
CREATE TABLE api_requests (
    request_id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    endpoint VARCHAR(200) NOT NULL,
    provider VARCHAR(50) NOT NULL,      -- polymarket, pinnacle, twitter, etc.
    status_code INTEGER,
    latency_ms INTEGER,
    error_message TEXT,
    timestamp TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_api_requests_provider ON api_requests(provider);
CREATE INDEX idx_api_requests_timestamp ON api_requests(timestamp);
SELECT create_hypertable('api_requests', 'timestamp', if_not_exists => TRUE);

-- ============================================================================
-- VIEWS
-- ============================================================================

-- Current portfolio summary
CREATE VIEW v_portfolio_summary AS
SELECT 
    total_capital,
    available_capital,
    invested_capital,
    unrealized_pnl,
    realized_pnl_today,
    daily_drawdown,
    open_positions,
    trades_today,
    timestamp
FROM portfolio_state
WHERE timestamp = (SELECT MAX(timestamp) FROM portfolio_state);

-- Active positions
CREATE VIEW v_active_positions AS
SELECT 
    t.trade_id,
    t.market_id,
    m.event_name,
    m.sport,
    t.strategy,
    t.position,
    t.quantity,
    t.entry_price,
    m.yes_price as current_price,
    (m.yes_price - t.entry_price) * t.quantity as unrealized_pnl,
    t.entry_time,
    EXTRACT(EPOCH FROM (NOW() - t.entry_time))/3600 as hours_held
FROM trades t
JOIN markets m ON t.market_id = m.market_id
WHERE t.status = 'open';

-- Strategy performance summary (last 30 days)
CREATE VIEW v_strategy_performance_30d AS
SELECT 
    strategy,
    SUM(trades_count) as total_trades,
    SUM(wins) as total_wins,
    SUM(losses) as total_losses,
    SUM(total_pnl) as total_pnl,
    AVG(win_rate) as avg_win_rate,
    AVG(profit_factor) as avg_profit_factor,
    AVG(sharpe_ratio) as avg_sharpe
FROM performance
WHERE date >= CURRENT_DATE - INTERVAL '30 days'
GROUP BY strategy;

-- ============================================================================
-- FUNCTIONS
-- ============================================================================

-- Function to update market prices
CREATE OR REPLACE FUNCTION update_market_price(
    p_market_id VARCHAR(66),
    p_yes_price DECIMAL(10, 8),
    p_no_price DECIMAL(10, 8),
    p_liquidity DECIMAL(20, 2)
)
RETURNS VOID AS $$
BEGIN
    UPDATE markets
    SET yes_price = p_yes_price,
        no_price = p_no_price,
        current_liquidity = p_liquidity,
        updated_at = NOW()
    WHERE market_id = p_market_id;
END;
$$ LANGUAGE plpgsql;

-- Function to calculate current portfolio state
CREATE OR REPLACE FUNCTION calculate_portfolio_state()
RETURNS TABLE (
    total_capital DECIMAL(20, 4),
    available_capital DECIMAL(20, 4),
    invested_capital DECIMAL(20, 4),
    unrealized_pnl DECIMAL(20, 4)
) AS $$
BEGIN
    RETURN QUERY
    WITH base_capital AS (
        SELECT 50000.00::DECIMAL(20, 4) as base  -- Starting capital
    ),
    realized AS (
        SELECT COALESCE(SUM(pnl), 0) as total_realized
        FROM trades
        WHERE status = 'closed'
    ),
    unrealized AS (
        SELECT COALESCE(SUM((m.yes_price - t.entry_price) * t.quantity), 0) as total_unrealized
        FROM trades t
        JOIN markets m ON t.market_id = m.market_id
        WHERE t.status = 'open'
    ),
    invested AS (
        SELECT COALESCE(SUM(t.entry_price * t.quantity), 0) as total_invested
        FROM trades t
        WHERE t.status = 'open'
    )
    SELECT 
        bc.base + r.total_realized as total_capital,
        bc.base + r.total_realized - i.total_invested as available_capital,
        i.total_invested as invested_capital,
        u.total_unrealized as unrealized_pnl
    FROM base_capital bc, realized r, unrealized u, invested i;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- TRIGGERS
-- ============================================================================

-- Auto-update updated_at timestamp
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER update_markets_updated_at BEFORE UPDATE ON markets
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- ============================================================================
-- INDEXES FOR OPTIMIZATION
-- ============================================================================

-- Composite indexes for common queries
CREATE INDEX idx_trades_strategy_entry_time ON trades(strategy, entry_time DESC);
CREATE INDEX idx_trades_market_status ON trades(market_id, status);
CREATE INDEX idx_signals_strategy_executed ON signals(strategy, executed, generated_at DESC);
CREATE INDEX idx_markets_sport_status_event_time ON markets(sport, status, event_time);

-- Partial indexes for active records
CREATE INDEX idx_markets_active ON markets(market_id) WHERE status = 'active';
CREATE INDEX idx_trades_open ON trades(trade_id) WHERE status = 'open';
CREATE INDEX idx_signals_pending ON signals(signal_id) WHERE executed = FALSE;
