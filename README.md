# Polymarket Sports Prediction Trading Bot v2.0

Production-ready automated trading system for Polymarket sports prediction markets targeting 30-50% annual returns with <15% maximum drawdown.

## 🎯 Overview

This trading bot implements 5 sophisticated strategies to generate alpha in sports prediction markets:

1. **CLV Arbitrage**: Exploit price differences between Polymarket and sharp bookmakers
2. **Poisson Expected Value**: Mathematical modeling of totals markets using Monte Carlo simulation
3. **Injury News Scalping**: React faster than market to breaking roster news
4. **Market Microstructure**: Follow smart money and whale wallet activity
5. **Sentiment Gap**: Fade extreme public sentiment using NLP analysis

## 🏗️ Architecture

```
┌─────────────────────────────────────────────────────┐
│                 5-Layer Architecture                 │
├─────────────────────────────────────────────────────┤
│  1. Data Ingestion     → Polymarket + Bookmakers    │
│  2. Signal Generation  → 5 Trading Strategies       │
│  3. Execution Engine   → Smart Order Routing        │
│  4. Risk Management    → Kelly + Circuit Breakers   │
│  5. Monitoring         → Prometheus + Grafana       │
└─────────────────────────────────────────────────────┘
```

## 📊 Technology Stack

- **Rust**: High-performance core engine (data ingestion, execution, risk management)
- **Python**: ML pipeline, sentiment analysis, backtesting
- **PostgreSQL + TimescaleDB**: Time-series data storage
- **Redis**: Message queue and caching
- **Docker**: Containerized deployment
- **Prometheus + Grafana**: Metrics and visualization

## 🚀 Quick Start

### Prerequisites

- Docker and Docker Compose
- Rust 1.75+ (for local development)
- Python 3.11+ (for ML pipeline development)
- PostgreSQL 15+ with TimescaleDB extension
- Polygon RPC endpoint (Alchemy/Infura)

### Installation

1. **Clone the repository**

```bash
cd /root/myproject/sports-prediction/polymarket-bot
```

2. **Set up environment variables**

```bash
cp .env.example .env
# Edit .env with your API keys and configuration
```

3. **Start with Docker Compose**

```bash
cd docker
docker-compose up -d
```

4. **Initialize the database**

```bash
docker-compose exec postgres psql -U trading_bot -d polymarket_bot -f /docker-entrypoint-initdb.d/01-schema.sql
```

5. **Access the services**

- Trading Bot Metrics: http://localhost:9090/metrics
- Grafana Dashboard: http://localhost:3000 (admin/admin)
- ML Pipeline API: http://localhost:8000/docs
- Prometheus: http://localhost:9090
- PgAdmin: http://localhost:5050 (dev profile only)

## 🔧 Configuration

### Main Configuration File

Edit `config/default.yaml` to customize:

- Strategy parameters (min edge, confidence thresholds)
- Risk limits (position sizing, drawdown limits)
- API endpoints and credentials
- Monitoring and alerting settings

### Environment Variables

Key environment variables (set in `.env`):

- `PRIVATE_KEY`: Your Ethereum/Polygon private key
- `POLYGON_RPC_URL`: Polygon RPC endpoint
- `DB_PASSWORD`: PostgreSQL password
- `TWITTER_BEARER_TOKEN`: Twitter API access
- `TELEGRAM_BOT_TOKEN`: For alerts

## 📈 Strategies

### 1. CLV Arbitrage

**Edge**: Polymarket prices often lag behind sharp bookmakers

- Tracks Pinnacle/Betfair closing lines
- Executes when divergence >3%
- Confidence proportional to edge size

### 2. Poisson Expected Value

**Edge**: Mathematical probability calculation for totals

- Monte Carlo simulation (10,000 iterations)
- Team offensive/defensive ratings
- Requires >5% edge and 95% confidence

### 3. Injury News Scalping

**Edge**: React faster than market to breaking news

- NLP classification of player importance
- Execution within 60 seconds
- Exit after market adjustment (5-15 min)

### 4. Market Microstructure

**Edge**: Follow informed trader activity

- Track whale wallet movements
- Analyze order flow patterns
- Trade with smart money direction

### 5. Sentiment Gap

**Edge**: Social media overreacts to news

- VADER + BERT sentiment analysis
- Fade extreme public sentiment
- Effective for popular teams/players

## 🛡️ Risk Management

### Position Sizing

- **Kelly Criterion**: Optimal position sizing with 0.5 fractional Kelly
- **Max Position**: 2% of portfolio per trade
- **Min Liquidity**: $5,000 per market

### Circuit Breakers

- **Daily Drawdown**: Halt at 8% daily loss
- **Consecutive Losses**: Cooldown after 3 losses
- **Trade Limits**: Maximum 20 trades per day
- **Correlation**: Max 0.6 between positions

### Risk Formulas

```
Position Size = min(2% portfolio, Kelly * 0.5)
Kelly = (bp - q) / b
  where: b = odds - 1, p = win probability, q = 1 - p
```

## 🧪 Backtesting

Run backtests with historical data:

```bash
# Using Python backtesting engine
cd python/backtesting
python backtest_engine.py --start-date 2024-01-01 --end-date 2024-12-31

# Using Rust backtest binary
cargo run --bin backtest -- --config config/default.yaml
```

### Backtest Assumptions

- **Slippage**: 1-3% depending on liquidity
- **Gas Costs**: $0.15 per transaction
- **Latency**: 500ms execution delay
- **Min Liquidity**: $2,000 filter

## 📊 Performance Targets

### Primary Metrics

- **Annualized Return**: 30-50% (after all costs)
- **Sharpe Ratio**: >1.5
- **Maximum Drawdown**: <15%
- **Win Rate**: >55%
- **Profit Factor**: >1.8

### Secondary Metrics

- **Strategy Correlation**: <0.3 between strategies
- **Alpha vs Benchmark**: >20%
- **Gas Efficiency**: <0.5% of PnL
- **Uptime**: >99.5%
- **Signal-to-Execution Latency**: <100ms

## 🔍 Monitoring

### Grafana Dashboards

Pre-configured dashboards include:

1. **Portfolio Overview**: Real-time P&L, capital allocation
2. **Strategy Performance**: Per-strategy metrics and attribution
3. **Risk Metrics**: Drawdown, correlation, VaR
4. **System Health**: Latency, uptime, error rates

### Alerting

Configure alerts via Telegram:

- Circuit breaker triggered
- Daily drawdown >6%
- System errors
- Abnormal gas prices
- Large winning/losing trades

## 🛠️ Development

### Local Development Setup

```bash
# Install Rust dependencies
cargo build

# Install Python dependencies
cd python
pip install -r requirements.txt

# Run tests
cargo test
pytest
```

### Project Structure

```
polymarket-bot/
├── src/                    # Rust source code
│   ├── main.rs            # Main entry point
│   ├── config.rs          # Configuration
│   ├── types.rs           # Type definitions
│   ├── data/              # Data ingestion
│   ├── strategies/        # Trading strategies
│   ├── execution/         # Trade execution
│   ├── risk/              # Risk management
│   └── monitoring/        # Metrics and monitoring
├── python/                # Python ML pipeline
│   ├── sentiment/         # Sentiment analysis
│   ├── backtesting/       # Backtesting engine
│   └── ml_pipeline/       # ML models
├── sql/                   # Database schema
├── config/                # Configuration files
├── docker/                # Docker configuration
└── README.md
```

## 🔐 Security

### Best Practices

- **Private Keys**: Store in environment variables, never commit to git
- **API Keys**: Rotate every 90 days
- **Multi-sig**: Use for large withdrawals
- **Rate Limiting**: Enabled on all external APIs
- **Auditing**: All trades logged to database

### Wallet Safety

- Never expose private keys in logs
- Use hardware wallet for key storage in production
- Implement withdrawal whitelisting
- Monitor unusual transaction patterns

## 📝 Database Schema

Key tables:

- **markets**: Polymarket market data
- **trades**: All executed trades
- **signals**: Generated trading signals
- **performance**: Daily performance metrics
- **bookmaker_odds**: Reference odds from sharp books
- **whale_wallets**: Tracked informed traders

See `sql/schema.sql` for complete schema.

## 🤝 Contributing

This is a production trading system. Code contributions should:

1. Include comprehensive tests
2. Follow Rust/Python best practices
3. Include performance benchmarks
4. Update documentation

## ⚠️ Disclaimer

This software is for educational and research purposes. Trading involves significant risk of loss. Use at your own risk. The authors are not responsible for any financial losses incurred.

## 📄 License

MIT License - See LICENSE file for details

## 📞 Support

For issues and questions:
- GitHub Issues: [Report bugs and feature requests]
- Documentation: See `/docs` directory
- Monitoring: Check Grafana dashboards for system health

## 🎯 Roadmap

- [ ] Additional strategies (momentum, mean reversion)
- [ ] More sports (NHL, tennis, esports)
- [ ] Advanced ML models (LSTM, transformers)
- [ ] Multi-chain support (Arbitrum, Optimism)
- [ ] Mobile app for monitoring
- [ ] Automated parameter optimization

---

**Built with ⚡ Rust and 🐍 Python for maximum performance and flexibility**

**Target: 30-50% annual returns | Max 15% drawdown | Sharpe >1.5**

## Contact Info
- Telegram: [@KratostesBoom](https://t.me/KratostesBoom)
- X: [@akratos_god](https://x.com/akratos_god)