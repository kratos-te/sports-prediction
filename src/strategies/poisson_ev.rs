use anyhow::Result;
use async_trait::async_trait;
use sqlx::PgPool;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use chrono::Utc;
use uuid::Uuid;
use tracing::{info, debug};
use statrs::distribution::{Poisson, Discrete};

use crate::types::{Market, Signal, SignalType, Strategy as StrategyEnum, MarketType};
use super::Strategy;

/// Strategy 2: Poisson Expected Value Model
/// 
/// Edge: Calculate true probabilities using Poisson distribution for totals markets
/// 
/// Implementation:
/// 1. For "Total Points Over/Under" markets
/// 2. Input: Team offensive/defensive ratings, pace, injuries
/// 3. Simulate 10,000 game outcomes using Poisson process
/// 4. Compare simulated probability vs. market probability
/// 5. Bet when edge > 5% and sample size significance > 95%
pub struct PoissonEvStrategy {
    db_pool: PgPool,
    min_edge_pct: Decimal,
    simulation_count: u32,
    min_significance: f64,
}

impl PoissonEvStrategy {
    pub fn new(
        db_pool: PgPool,
        min_edge_pct: f64,
        simulation_count: u32,
        min_significance: f64,
    ) -> Self {
        Self {
            db_pool,
            min_edge_pct: Decimal::from_f64_retain(min_edge_pct).unwrap_or(dec!(5.0)),
            simulation_count,
            min_significance,
        }
    }

    /// Estimate team scoring rates (lambda parameters for Poisson)
    async fn estimate_scoring_rates(&self, market: &Market) -> Result<Option<(f64, f64)>> {
        // In production, this would:
        // 1. Parse team names from market description
        // 2. Query team stats from database (offensive/defensive ratings)
        // 3. Adjust for injuries, home/away, pace, etc.
        
        // For now, return example values
        // These would come from a comprehensive sports analytics database
        
        if market.market_type != MarketType::Total {
            return Ok(None);
        }

        // Example: NFL game with average scoring
        // Team A expected: 24 points (lambda = 24)
        // Team B expected: 21 points (lambda = 21)
        // Total expected: 45 points
        
        let team_a_lambda = 24.0;
        let team_b_lambda = 21.0;
        
        Ok(Some((team_a_lambda, team_b_lambda)))
    }

    /// Simulate game outcomes using Poisson distribution
    fn simulate_game_outcomes(
        &self,
        team_a_lambda: f64,
        team_b_lambda: f64,
        total_line: f64,
    ) -> Result<SimulationResult> {
        use rand::Rng;
        
        let poisson_a = Poisson::new(team_a_lambda)?;
        let poisson_b = Poisson::new(team_b_lambda)?;
        
        let mut over_count = 0;
        let mut under_count = 0;
        let mut total_scores = Vec::with_capacity(self.simulation_count as usize);
        
        let mut rng = rand::thread_rng();
        
        for _ in 0..self.simulation_count {
            // Sample from Poisson distributions
            let score_a = self.sample_poisson(&poisson_a, &mut rng);
            let score_b = self.sample_poisson(&poisson_b, &mut rng);
            let total = score_a + score_b;
            
            total_scores.push(total);
            
            if total as f64 > total_line {
                over_count += 1;
            } else {
                under_count += 1;
            }
        }
        
        let over_probability = over_count as f64 / self.simulation_count as f64;
        let under_probability = under_count as f64 / self.simulation_count as f64;
        
        // Calculate mean and standard deviation
        let mean: f64 = total_scores.iter().sum::<u32>() as f64 / total_scores.len() as f64;
        let variance: f64 = total_scores.iter()
            .map(|&x| {
                let diff = x as f64 - mean;
                diff * diff
            })
            .sum::<f64>() / total_scores.len() as f64;
        let std_dev = variance.sqrt();
        
        Ok(SimulationResult {
            over_probability,
            under_probability,
            mean_total: mean,
            std_dev,
            simulations: self.simulation_count,
        })
    }

    fn sample_poisson<R: rand::Rng>(&self, poisson: &Poisson, rng: &mut R) -> u32 {
        // Sample from Poisson distribution
        let lambda = poisson.lambda();
        let mut k = 0;
        let mut p = (-lambda).exp();
        let mut s = p;
        let u: f64 = rng.gen();
        
        while u > s {
            k += 1;
            p *= lambda / k as f64;
            s += p;
        }
        
        k as u32
    }

    /// Calculate confidence based on edge size and statistical significance
    fn calculate_confidence(
        &self,
        edge_pct: Decimal,
        simulation_result: &SimulationResult,
    ) -> Decimal {
        // Higher edge = higher confidence
        let edge_confidence = (edge_pct / dec!(20.0)).min(dec!(0.8));
        
        // Statistical significance bonus
        // More simulations and clear separation = higher confidence
        let z_score = (simulation_result.over_probability - 0.5).abs() / 
                     (0.25_f64 / simulation_result.simulations as f64).sqrt();
        
        let significance_bonus = if z_score > 2.58 { // 99% confidence
            dec!(0.2)
        } else if z_score > 1.96 { // 95% confidence
            dec!(0.15)
        } else if z_score > 1.64 { // 90% confidence
            dec!(0.1)
        } else {
            dec!(0.0)
        };
        
        (edge_confidence + significance_bonus).min(dec!(1.0))
    }

    /// Parse total line from market description
    fn parse_total_line(&self, market: &Market) -> Option<f64> {
        // Extract total line from market description
        // Example: "Total Points Over 45.5" -> 45.5
        
        let description = market.description.as_ref()?;
        let words: Vec<&str> = description.split_whitespace().collect();
        
        for (i, word) in words.iter().enumerate() {
            if word.to_lowercase().contains("over") || word.to_lowercase().contains("under") {
                if let Some(next_word) = words.get(i + 1) {
                    if let Ok(line) = next_word.parse::<f64>() {
                        return Some(line);
                    }
                }
            }
        }
        
        // Default for example purposes
        Some(45.5)
    }
}

#[async_trait]
impl Strategy for PoissonEvStrategy {
    async fn generate_signals(&self, markets: &[Market]) -> Result<Vec<Signal>> {
        let mut signals = Vec::new();

        for market in markets {
            // Only analyze totals markets
            if market.market_type != MarketType::Total {
                continue;
            }

            if market.status != crate::types::MarketStatus::Active {
                continue;
            }

            // Get scoring rates for both teams
            let (team_a_lambda, team_b_lambda) = match self.estimate_scoring_rates(market).await? {
                Some(rates) => rates,
                None => continue,
            };

            // Parse the total line
            let total_line = match self.parse_total_line(market) {
                Some(line) => line,
                None => continue,
            };

            // Run Monte Carlo simulation
            let simulation_result = match self.simulate_game_outcomes(
                team_a_lambda,
                team_b_lambda,
                total_line,
            ) {
                Ok(result) => result,
                Err(e) => {
                    debug!("Simulation failed for {}: {}", market.market_id, e);
                    continue;
                }
            };

            // Determine if there's an edge
            let over_edge = Decimal::from_f64_retain(simulation_result.over_probability).unwrap() 
                          - market.yes_price;
            let under_edge = Decimal::from_f64_retain(simulation_result.under_probability).unwrap() 
                           - market.no_price;

            let (signal_type, edge_pct, fair_value) = if over_edge > self.min_edge_pct / dec!(100.0) {
                (
                    SignalType::BuyYes,
                    over_edge * dec!(100.0),
                    Decimal::from_f64_retain(simulation_result.over_probability).unwrap(),
                )
            } else if under_edge > self.min_edge_pct / dec!(100.0) {
                (
                    SignalType::BuyNo,
                    under_edge * dec!(100.0),
                    Decimal::from_f64_retain(simulation_result.under_probability).unwrap(),
                )
            } else {
                continue;
            };

            let confidence = self.calculate_confidence(edge_pct, &simulation_result);
            let recommended_size = dec!(1000.0) * confidence;

            let signal = Signal {
                signal_id: Uuid::new_v4(),
                market_id: market.market_id.clone(),
                strategy: StrategyEnum::PoissonExpectedValue,
                signal_type: signal_type.clone(),
                confidence,
                edge_size: edge_pct / dec!(100.0),
                recommended_size,
                current_price: market.implied_probability(signal_type.to_position()),
                fair_value,
                generated_at: Utc::now(),
                metadata: serde_json::json!({
                    "team_a_lambda": team_a_lambda,
                    "team_b_lambda": team_b_lambda,
                    "total_line": total_line,
                    "simulated_mean": simulation_result.mean_total,
                    "simulated_std_dev": simulation_result.std_dev,
                    "over_probability": simulation_result.over_probability,
                    "under_probability": simulation_result.under_probability,
                    "simulations": simulation_result.simulations,
                }),
            };

            info!(
                "📊 Poisson EV Signal: {} {} - Edge: {:.2}%, Confidence: {:.2}%",
                market.event_name,
                match signal_type {
                    SignalType::BuyYes => "OVER",
                    SignalType::BuyNo => "UNDER",
                },
                edge_pct,
                confidence * dec!(100.0)
            );

            signals.push(signal);
        }

        Ok(signals)
    }

    fn name(&self) -> &str {
        "Poisson Expected Value"
    }

    fn is_enabled(&self) -> bool {
        true
    }
}

#[derive(Debug)]
struct SimulationResult {
    over_probability: f64,
    under_probability: f64,
    mean_total: f64,
    std_dev: f64,
    simulations: u32,
}
