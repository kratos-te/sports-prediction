# Deployment Guide - Polymarket Trading Bot

## Production Deployment Checklist

### Pre-Deployment

- [ ] Test all strategies in paper trading mode
- [ ] Run comprehensive backtests (min 6 months historical data)
- [ ] Verify all API keys and credentials
- [ ] Set up monitoring and alerting
- [ ] Configure risk limits appropriately
- [ ] Fund wallet with initial capital
- [ ] Test circuit breakers manually

### Infrastructure Setup

#### 1. Cloud Provider Setup (AWS/GCP)

**AWS Recommended Resources:**
- **Compute**: ECS Fargate or EC2 t3.medium
- **Database**: RDS PostgreSQL with TimescaleDB
- **Cache**: ElastiCache Redis
- **Secrets**: AWS Secrets Manager
- **Monitoring**: CloudWatch + Prometheus

**Estimated Monthly Costs:**
- Compute: $50-100
- Database: $50-80
- Redis: $15-30
- Networking: $10-20
- **Total: ~$125-230/month**

#### 2. Database Setup

```bash
# Create RDS PostgreSQL instance
aws rds create-db-instance \
    --db-instance-identifier polymarket-trading-db \
    --db-instance-class db.t3.medium \
    --engine postgres \
    --engine-version 15.4 \
    --allocated-storage 50 \
    --master-username trading_bot \
    --master-user-password <PASSWORD>

# Install TimescaleDB extension
psql -h <RDS_ENDPOINT> -U trading_bot -d polymarket_bot \
     -c "CREATE EXTENSION IF NOT EXISTS timescaledb;"

# Run migrations
psql -h <RDS_ENDPOINT> -U trading_bot -d polymarket_bot \
     -f sql/schema.sql
```

#### 3. Container Deployment

**Using Docker Swarm:**

```bash
# Initialize swarm
docker swarm init

# Deploy stack
docker stack deploy -c docker/docker-compose.yml polymarket

# Check services
docker service ls
```

**Using Kubernetes:**

```bash
# Create namespace
kubectl create namespace polymarket-bot

# Create secrets
kubectl create secret generic bot-secrets \
    --from-env-file=.env \
    -n polymarket-bot

# Deploy
kubectl apply -f k8s/ -n polymarket-bot
```

#### 4. Environment Variables (Production)

```bash
# Blockchain
PRIVATE_KEY=<SECURE_KEY_FROM_SECRETS_MANAGER>
POLYGON_RPC_URL=<PRIMARY_ALCHEMY_URL>
POLYGON_RPC_URL_BACKUP=<BACKUP_INFURA_URL>

# Database
DATABASE_URL=<RDS_ENDPOINT>
DB_PASSWORD=<SECURE_PASSWORD>
DB_MAX_CONNECTIONS=20

# Redis
REDIS_URL=<ELASTICACHE_ENDPOINT>

# Monitoring
GRAFANA_ADMIN_PASSWORD=<SECURE_PASSWORD>
TELEGRAM_BOT_TOKEN=<BOT_TOKEN>
TELEGRAM_CHAT_ID=<YOUR_CHAT_ID>

# Application
ENVIRONMENT=production
RUST_LOG=info,trading_bot=debug
```

### Security Hardening

#### 1. Network Security

```bash
# Configure Security Groups (AWS)
# Allow only necessary ports:
# - 22 (SSH) from your IP only
# - 443 (HTTPS) for APIs
# - Internal VPC communication

# Example AWS security group
aws ec2 create-security-group \
    --group-name polymarket-bot-sg \
    --description "Trading bot security group"
```

#### 2. Secrets Management

```bash
# Store private key in Secrets Manager
aws secretsmanager create-secret \
    --name polymarket-bot/private-key \
    --secret-string <PRIVATE_KEY>

# Rotate secrets every 90 days
aws secretsmanager rotate-secret \
    --secret-id polymarket-bot/private-key \
    --rotation-rules AutomaticallyAfterDays=90
```

#### 3. Wallet Security

- Use hardware wallet (Ledger/Trezor) for key generation
- Implement multi-sig for withdrawals >$10,000
- Whitelist withdrawal addresses
- Monitor for unusual transaction patterns
- Keep minimum funds in hot wallet

### Monitoring Setup

#### 1. Prometheus Alerts

Create `prometheus-alerts.yml`:

```yaml
groups:
  - name: trading_bot_alerts
    interval: 30s
    rules:
      - alert: HighDailyDrawdown
        expr: daily_drawdown_pct > 6.0
        for: 1m
        annotations:
          summary: "Daily drawdown exceeds 6%"
      
      - alert: TradingBotDown
        expr: up{job="trading-bot"} == 0
        for: 5m
        annotations:
          summary: "Trading bot is down"
      
      - alert: HighGasPrices
        expr: gas_price_gwei > 100
        for: 10m
        annotations:
          summary: "Gas prices above 100 gwei"
```

#### 2. Grafana Dashboards

Import pre-configured dashboards:
1. Portfolio Performance
2. Strategy Attribution
3. Risk Metrics
4. System Health

#### 3. Telegram Alerts

Configure critical alerts:
- Circuit breaker triggered
- System errors
- Large trades (>$1,000)
- Daily PnL updates

### Performance Optimization

#### 1. Database Optimization

```sql
-- Create additional indexes for common queries
CREATE INDEX idx_trades_pnl ON trades(pnl) WHERE status = 'closed';
CREATE INDEX idx_signals_confidence ON signals(confidence DESC, edge_size DESC);

-- Enable query performance insights
ALTER DATABASE polymarket_bot SET track_io_timing = ON;
ALTER DATABASE polymarket_bot SET track_functions = 'all';

-- Vacuum and analyze regularly
VACUUM ANALYZE trades;
VACUUM ANALYZE markets;
VACUUM ANALYZE signals;
```

#### 2. Redis Caching

```python
# Cache frequently accessed data
# - Market prices (TTL: 5 seconds)
# - Bookmaker odds (TTL: 30 seconds)
# - Whale wallet activity (TTL: 60 seconds)
```

#### 3. Connection Pooling

```yaml
database:
  max_connections: 20
  min_connections: 5
  connection_timeout: 10
  idle_timeout: 300
```

### Backup Strategy

#### 1. Database Backups

```bash
# Automated daily backups
aws rds create-db-snapshot \
    --db-instance-identifier polymarket-trading-db \
    --db-snapshot-identifier polymarket-backup-$(date +%Y%m%d)

# Retention: 30 days
# Schedule: Daily at 2 AM UTC
```

#### 2. Configuration Backups

```bash
# Backup config to S3
aws s3 sync config/ s3://polymarket-bot-backups/config/ \
    --exclude ".env"

# Backup crontab: Daily
```

#### 3. Trade History Export

```sql
-- Export trade history monthly
COPY (
    SELECT * FROM trades 
    WHERE DATE_TRUNC('month', entry_time) = DATE_TRUNC('month', CURRENT_DATE - INTERVAL '1 month')
) TO '/tmp/trades_export.csv' WITH CSV HEADER;
```

### Disaster Recovery

#### Recovery Time Objective (RTO): 1 hour
#### Recovery Point Objective (RPO): 5 minutes

**Recovery Steps:**

1. **Database Failure**
   ```bash
   # Restore from latest snapshot
   aws rds restore-db-instance-from-db-snapshot \
       --db-instance-identifier polymarket-trading-db-restore \
       --db-snapshot-identifier <LATEST_SNAPSHOT>
   ```

2. **Application Failure**
   ```bash
   # Redeploy from latest Docker image
   docker service update --image polymarket-bot:latest trading-bot
   ```

3. **Complete Infrastructure Loss**
   - Restore database from S3 backup
   - Redeploy using Infrastructure as Code (Terraform)
   - Restore configuration from git
   - Resume trading after validation

### Scaling Guidelines

#### When to Scale Up:

- **Database**: >80% CPU or Memory utilization
- **Trading Bot**: Processing latency >100ms
- **Redis**: >70% memory usage

#### Horizontal Scaling:

```yaml
# Scale bot replicas (Docker Swarm)
docker service scale polymarket_trading-bot=3

# Or Kubernetes
kubectl scale deployment trading-bot --replicas=3 -n polymarket-bot
```

### Cost Optimization

1. **Use Spot/Preemptible Instances** for non-critical workloads
2. **Right-size Resources** based on actual usage
3. **Reserved Instances** for database (save 30-40%)
4. **Compress Logs** older than 7 days
5. **Archive Historical Data** to S3 Glacier (>90 days)

### Maintenance Schedule

**Daily:**
- Review trading performance
- Check error logs
- Verify all services are healthy

**Weekly:**
- Analyze strategy performance
- Review and adjust risk parameters
- Database maintenance (VACUUM, ANALYZE)
- Update bookmaker odds sources

**Monthly:**
- Rotate API keys and credentials
- Review and optimize strategies
- Backup and archive old data
- Security audit
- Cost analysis

**Quarterly:**
- Full backtesting with updated data
- Strategy parameter optimization
- Infrastructure security review
- Disaster recovery drill

### Troubleshooting

#### Bot Not Executing Trades

1. Check circuit breaker status:
   ```sql
   SELECT * FROM circuit_breakers WHERE status = 'active';
   ```

2. Verify wallet balance and gas prices

3. Check risk limits haven't been breached

4. Review recent error logs

#### High Latency

1. Check database connection pool
2. Verify Redis connectivity
3. Monitor network latency to Polygon RPC
4. Review slow query log

#### Unexpected Losses

1. Review trade history and attribution
2. Check if circuit breaker triggered
3. Analyze strategy performance
4. Verify bookmaker data quality
5. Check for unusual market conditions

### Support and Monitoring

**24/7 Monitoring:**
- Uptime monitoring (UptimeRobot)
- Error tracking (Sentry)
- Performance monitoring (Datadog/New Relic)

**Alerts:**
- PagerDuty for critical issues
- Telegram for warnings
- Email for daily summaries

**Health Check Endpoint:**
```
GET /health
{
  "status": "healthy",
  "uptime": 86400,
  "open_positions": 5,
  "daily_pnl": 450.25,
  "circuit_breaker": false
}
```

### Compliance and Legal

- **Record Keeping**: Maintain all trade records for 7 years
- **Tax Reporting**: Export trade data for tax purposes
- **Regulatory Compliance**: Ensure compliance with local trading regulations
- **Terms of Service**: Review Polymarket ToS regularly

---

**Remember:** Start with paper trading, gradually increase capital, and always monitor closely during the first month of live trading.
