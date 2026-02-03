use anyhow::Result;
use async_trait::async_trait;
use sqlx::PgPool;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use chrono::Utc;
use uuid::Uuid;
use tracing::{info, debug};

use crate::types::{Market, Signal, SignalType, Strategy as StrategyEnum, Position, BookmakerOdds};
use super::Strategy;

/// Strategy 1: Closing Line Value (CLV) Arbitrage
/// 
/// Edge: Polymarket prices often lag behind sharp bookmaker line movements
/// 
/// Implementation:
/// 1. Track Pinnacle/Betfair closing lines for each game
/// 2. Calculate implied probabilities from moneyline/point spreads
/// 3. When Polymarket probability diverges by >3% from sharp books
/// 4. Execute with confidence proportional to divergence
/// 5. Exit position as markets converge (or hold to resolution)
pub struct ClvArbitrageStrategy {
    db_pool: PgPool,
    min_divergence_pct: Decimal,
    max_hold_hours: i64,
}

impl ClvArbitrageStrategy {
    pub fn new(
        db_pool: PgPool,
        min_divergence_pct: f64,
        max_hold_hours: u64,
    ) -> Self {
        Self {
            db_pool,
            min_divergence_pct: Decimal::from_f64_retain(min_divergence_pct).unwrap_or(dec!(3.0)),
            max_hold_hours: max_hold_hours as i64,
        }
    }

    /// Fetch latest bookmaker odds for a market
    async fn fetch_bookmaker_odds(&self, market_id: &str) -> Result<Vec<BookmakerOdds>> {
        let odds = sqlx::query_as!(
            BookmakerOddsRow,
            r#"
            SELECT DISTINCT ON (bookmaker)
                market_id,
                bookmaker,
                yes_odds,
                no_odds,
                yes_implied_prob,
                no_implied_prob,
                timestamp
            FROM bookmaker_odds
            WHERE market_id = $1
                AND timestamp > NOW() - INTERVAL '1 hour'
            ORDER BY bookmaker, timestamp DESC
            "#,
            market_id
        )
        .fetch_all(&self.db_pool)
        .await?;

        Ok(odds.into_iter().map(|row| row.into()).collect())
    }

    /// Calculate the fair value based on sharp bookmaker odds
    fn calculate_fair_value(&self, bookmaker_odds: &[BookmakerOdds]) -> Option<(Decimal, Decimal)> {
        if bookmaker_odds.is_empty() {
            return None;
        }

        // Weight Pinnacle heavily as they're the sharpest
        let mut yes_prob_sum = dec!(0.0);
        let mut no_prob_sum = dec!(0.0);
        let mut weight_sum = dec!(0.0);

        for odds in bookmaker_odds {
            let weight = match odds.bookmaker {
                crate::types::Bookmaker::Pinnacle => dec!(2.0), // 2x weight for Pinnacle
                crate::types::Bookmaker::Betfair => dec!(1.5),
                _ => dec!(1.0),
            };

            yes_prob_sum += odds.yes_implied_prob * weight;
            no_prob_sum += odds.no_implied_prob * weight;
            weight_sum += weight;
        }

        if weight_sum > dec!(0.0) {
            let fair_yes = yes_prob_sum / weight_sum;
            let fair_no = no_prob_sum / weight_sum;
            
            // Normalize to sum to 1.0 (remove vig)
            let total = fair_yes + fair_no;
            if total > dec!(0.0) {
                return Some((fair_yes / total, fair_no / total));
            }
        }

        None
    }

    /// Calculate confidence based on divergence size and data quality
    fn calculate_confidence(
        &self,
        divergence: Decimal,
        num_bookmakers: usize,
    ) -> Decimal {
        use rust_decimal_macros::dec;
        
        // Base confidence from divergence (larger divergence = higher confidence)
        let divergence_confidence = (divergence / dec!(10.0)).min(dec!(0.7));
        
        // Data quality bonus (more bookmakers = more confident)
        let data_quality_bonus = match num_bookmakers {
            0..=1 => dec!(0.0),
            2 => dec!(0.1),
            3 => dec!(0.15),
            _ => dec!(0.2),
        };
        
        (divergence_confidence + data_quality_bonus).min(dec!(1.0))
    }

    /// Determine signal type based on which side is underpriced
    fn determine_signal_type(
        &self,
        market: &Market,
        fair_yes: Decimal,
        fair_no: Decimal,
    ) -> Option<(SignalType, Decimal, Decimal)> {
        let yes_divergence = fair_yes - market.yes_price;
        let no_divergence = fair_no - market.no_price;

        // Check if YES is underpriced (market price < fair value)
        if yes_divergence > self.min_divergence_pct / dec!(100.0) {
            return Some((SignalType::BuyYes, yes_divergence * dec!(100.0), fair_yes));
        }

        // Check if NO is underpriced
        if no_divergence > self.min_divergence_pct / dec!(100.0) {
            return Some((SignalType::BuyNo, no_divergence * dec!(100.0), fair_no));
        }

        None
    }
}

#[async_trait]
impl Strategy for ClvArbitrageStrategy {
    async fn generate_signals(&self, markets: &[Market]) -> Result<Vec<Signal>> {
        let mut signals = Vec::new();

        for market in markets {
            // Only analyze active markets with sufficient liquidity
            if market.status != crate::types::MarketStatus::Active {
                continue;
            }

            // Fetch bookmaker odds
            let bookmaker_odds = match self.fetch_bookmaker_odds(&market.market_id).await {
                Ok(odds) => odds,
                Err(e) => {
                    debug!("Failed to fetch bookmaker odds for {}: {}", market.market_id, e);
                    continue;
                }
            };

            if bookmaker_odds.is_empty() {
                continue;
            }

            // Calculate fair value from bookmaker odds
            let (fair_yes, fair_no) = match self.calculate_fair_value(&bookmaker_odds) {
                Some(values) => values,
                None => continue,
            };

            // Determine if there's a signal
            if let Some((signal_type, edge_pct, fair_value)) = 
                self.determine_signal_type(market, fair_yes, fair_no)
            {
                let confidence = self.calculate_confidence(edge_pct, bookmaker_odds.len());
                
                // Calculate recommended position size (will be adjusted by risk management)
                let recommended_size = dec!(1000.0) * confidence; // Base size * confidence

                let signal = Signal {
                    signal_id: Uuid::new_v4(),
                    market_id: market.market_id.clone(),
                    strategy: StrategyEnum::ClvArbitrage,
                    signal_type: signal_type.clone(),
                    confidence,
                    edge_size: edge_pct / dec!(100.0),
                    recommended_size,
                    current_price: market.implied_probability(signal_type.to_position()),
                    fair_value,
                    generated_at: Utc::now(),
                    metadata: serde_json::json!({
                        "num_bookmakers": bookmaker_odds.len(),
                        "fair_yes": fair_yes,
                        "fair_no": fair_no,
                        "market_yes": market.yes_price,
                        "market_no": market.no_price,
                    }),
                };

                info!(
                    "🎯 CLV Signal: {} {} - Edge: {:.2}%, Confidence: {:.2}",
                    market.event_name,
                    match signal_type {
                        SignalType::BuyYes => "YES",
                        SignalType::BuyNo => "NO",
                    },
                    edge_pct,
                    confidence * dec!(100.0)
                );

                signals.push(signal);
            }
        }

        Ok(signals)
    }

    fn name(&self) -> &str {
        "CLV Arbitrage"
    }

    fn is_enabled(&self) -> bool {
        true
    }
}

// Helper struct for database queries
#[derive(Debug)]
struct BookmakerOddsRow {
    market_id: String,
    bookmaker: String,
    yes_odds: Decimal,
    no_odds: Decimal,
    yes_implied_prob: Decimal,
    no_implied_prob: Decimal,
    timestamp: chrono::DateTime<Utc>,
}

impl From<BookmakerOddsRow> for BookmakerOdds {
    fn from(row: BookmakerOddsRow) -> Self {
        use crate::types::Bookmaker;
        
        let bookmaker = match row.bookmaker.as_str() {
            "pinnacle" => Bookmaker::Pinnacle,
            "betfair" => Bookmaker::Betfair,
            "draftkings" => Bookmaker::DraftKings,
            _ => Bookmaker::Pinnacle,
        };

        BookmakerOdds {
            bookmaker,
            market_id: row.market_id,
            yes_odds: row.yes_odds,
            no_odds: row.no_odds,
            yes_implied_prob: row.yes_implied_prob,
            no_implied_prob: row.no_implied_prob,
            timestamp: row.timestamp,
        }
    }
}
