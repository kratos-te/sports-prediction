# Quick Start Guide - Polymarket Trading Bot

Get your trading bot up and running in 15 minutes!

## Prerequisites

- Docker and Docker Compose installed
- A Polygon wallet with private key
- Polygon RPC endpoint (get free at Alchemy or Infura)
- At least 0.5 MATIC for gas fees (optional for testing)

## Step 1: Clone and Configure

```bash
cd /root/myproject/sports-prediction/polymarket-bot

# Copy environment template
cp .env.example .env
```

## Step 2: Edit Configuration

Edit `.env` file with your credentials:

```bash
nano .env
```

**Minimum required:**
```env
DB_PASSWORD=secure_password_123
PRIVATE_KEY=0x...your_private_key...
POLYGON_RPC_URL=https://polygon-mainnet.g.alchemy.com/v2/YOUR_KEY
POLYGON_WS_URL=wss://polygon-mainnet.g.alchemy.com/v2/YOUR_KEY
```

## Step 3: Start Services

```bash
cd docker
docker-compose up -d
```

Wait 30-60 seconds for all services to start.

## Step 4: Initialize Database

```bash
docker-compose exec postgres psql -U trading_bot -d polymarket_bot -f /docker-entrypoint-initdb.d/01-schema.sql
```

## Step 5: Verify Services

Check that all services are running:

```bash
docker-compose ps
```

You should see:
- ✅ postgres (healthy)
- ✅ redis (healthy)
- ✅ trading-bot (running)
- ✅ ml-pipeline (running)
- ✅ prometheus (running)
- ✅ grafana (running)

## Step 6: Access Dashboards

Open in your browser:

1. **Grafana Dashboard**: http://localhost:3000
   - Username: `admin`
   - Password: `admin` (or what you set in .env)

2. **ML Pipeline API**: http://localhost:8000/docs
   - Interactive API documentation

3. **Prometheus**: http://localhost:9090
   - Raw metrics

## Step 7: Monitor Trading

### View Logs

```bash
# Trading bot logs
docker-compose logs -f trading-bot

# All services
docker-compose logs -f
```

### Check Portfolio

```bash
docker-compose exec postgres psql -U trading_bot -d polymarket_bot -c "SELECT * FROM v_portfolio_summary;"
```

### View Active Positions

```bash
docker-compose exec postgres psql -U trading_bot -d polymarket_bot -c "SELECT * FROM v_active_positions;"
```

## Step 8: Paper Trading (Recommended First)

Before using real funds, test with paper trading:

1. Edit `config/default.yaml`:
   ```yaml
   paper_trading: true
   ```

2. Restart the bot:
   ```bash
   docker-compose restart trading-bot
   ```

## Common Commands

### Restart Bot
```bash
docker-compose restart trading-bot
```

### View Database
```bash
docker-compose exec postgres psql -U trading_bot -d polymarket_bot
```

### Stop Everything
```bash
docker-compose down
```

### Update Bot
```bash
git pull
docker-compose build
docker-compose up -d
```

## Troubleshooting

### Bot Not Starting

1. Check logs:
   ```bash
   docker-compose logs trading-bot
   ```

2. Verify environment variables:
   ```bash
   docker-compose exec trading-bot env | grep TRADING_BOT
   ```

### Database Connection Failed

```bash
# Check database is running
docker-compose ps postgres

# Restart database
docker-compose restart postgres
```

### No Trades Executing

1. Check circuit breakers:
   ```sql
   SELECT * FROM circuit_breakers WHERE status = 'active';
   ```

2. Verify risk limits:
   ```sql
   SELECT * FROM risk_limits;
   ```

3. Check available capital:
   ```sql
   SELECT * FROM v_portfolio_summary;
   ```

## Safety Checklist

Before going live with real money:

- [ ] Tested in paper trading mode for at least 7 days
- [ ] Reviewed all strategy parameters
- [ ] Set appropriate risk limits
- [ ] Configured Telegram alerts
- [ ] Verified wallet has enough MATIC for gas
- [ ] Backed up private key securely
- [ ] Set up monitoring dashboard
- [ ] Tested circuit breakers manually

## Key Configuration Parameters

Edit `config/default.yaml` to adjust:

### Risk Limits
```yaml
risk:
  starting_capital: 50000.0      # Starting capital
  max_position_size_pct: 2.0     # Max 2% per trade
  daily_drawdown_limit_pct: 8.0  # Stop at 8% daily loss
  max_daily_trades: 20           # Max trades per day
```

### Strategies
```yaml
strategies:
  enabled_strategies:
    - "clv_arb"          # CLV Arbitrage
    - "poisson_ev"       # Poisson Expected Value
```

### Strategy Parameters
```yaml
clv_arb:
  min_divergence_pct: 3.0  # Min 3% edge required

poisson_ev:
  min_edge_pct: 5.0        # Min 5% edge required
```

## Next Steps

1. **Monitor Performance**: Watch the Grafana dashboard daily
2. **Review Trades**: Analyze winning and losing trades
3. **Adjust Parameters**: Fine-tune based on performance
4. **Scale Gradually**: Increase capital as confidence grows
5. **Stay Informed**: Keep up with Polymarket and sports news

## Performance Targets

- **Win Rate**: Target >55%
- **Sharpe Ratio**: Target >1.5
- **Max Drawdown**: Keep <15%
- **Annual Return**: Target 30-50%

## Support

- **Documentation**: See README.md and ARCHITECTURE.md
- **Deployment**: See DEPLOYMENT.md for production setup
- **Logs**: Check `docker-compose logs` for errors
- **Database**: Use PgAdmin (port 5050) for database inspection

## Important Notes

⚠️ **Warning**: 
- Start with small amounts
- Always use paper trading first
- Trading involves risk of loss
- Monitor the bot regularly
- Have a stop-loss plan

🎯 **Remember**:
- The bot requires active monitoring
- Past performance doesn't guarantee future results
- Always maintain sufficient gas funds
- Keep your private key secure

---

**You're all set! The bot is now analyzing markets and will execute trades based on your configured strategies. Happy trading! 🚀**
