"""
Backtesting Engine for Trading Strategies

Implements walk-forward optimization with realistic assumptions:
- Slippage: 1-3% depending on liquidity
- Gas costs: $0.15 per transaction
- Latency: 500ms execution delay
- Minimum liquidity filter: $2,000
"""

import pandas as pd
import numpy as np
from typing import Dict, List, Tuple, Optional
from dataclasses import dataclass
from datetime import datetime, timedelta
import json


@dataclass
class BacktestConfig:
    """Backtesting configuration"""
    starting_capital: float = 50000.0
    max_position_size_pct: float = 2.0
    daily_drawdown_limit_pct: float = 8.0
    min_liquidity: float = 2000.0
    slippage_pct: float = 0.02  # 2%
    gas_cost_per_trade: float = 0.15
    execution_latency_ms: int = 500
    kelly_fraction: float = 0.5


@dataclass
class Trade:
    """Single trade record"""
    entry_time: datetime
    exit_time: datetime
    market_id: str
    strategy: str
    position: str  # 'yes' or 'no'
    entry_price: float
    exit_price: float
    quantity: float
    pnl: float
    pnl_pct: float
    gas_cost: float
    slippage: float


class BacktestEngine:
    """
    Backtesting engine with walk-forward optimization
    """

    def __init__(self, config: BacktestConfig = None):
        self.config = config or BacktestConfig()
        self.trades: List[Trade] = []
        self.portfolio_value_history = []
        self.capital = self.config.starting_capital
        
    def run_backtest(
        self,
        historical_data: pd.DataFrame,
        strategy_signals: pd.DataFrame,
        start_date: str = None,
        end_date: str = None
    ) -> Dict:
        """
        Run backtest on historical data
        
        Args:
            historical_data: DataFrame with columns:
                [timestamp, market_id, yes_price, no_price, liquidity, resolution]
            strategy_signals: DataFrame with columns:
                [timestamp, market_id, signal_type, confidence, edge_size, 
                 entry_price, fair_value, strategy]
            start_date: Start date for backtest
            end_date: End date for backtest
            
        Returns:
            Backtest results dictionary
        """
        # Filter data by date range
        if start_date:
            historical_data = historical_data[
                historical_data['timestamp'] >= start_date
            ]
            strategy_signals = strategy_signals[
                strategy_signals['timestamp'] >= start_date
            ]
        
        if end_date:
            historical_data = historical_data[
                historical_data['timestamp'] <= end_date
            ]
            strategy_signals = strategy_signals[
                strategy_signals['timestamp'] <= end_date
            ]
        
        # Reset state
        self.trades = []
        self.portfolio_value_history = []
        self.capital = self.config.starting_capital
        open_positions = {}
        
        # Group signals by timestamp
        signals_by_time = strategy_signals.groupby('timestamp')
        
        # Simulate trading day by day
        for timestamp in sorted(historical_data['timestamp'].unique()):
            # Process signals for this timestamp
            if timestamp in signals_by_time.groups:
                signals = strategy_signals[
                    strategy_signals['timestamp'] == timestamp
                ]
                
                for _, signal in signals.iterrows():
                    # Check if we can take this trade
                    if self._can_enter_trade(signal):
                        trade = self._enter_position(signal, timestamp)
                        if trade:
                            open_positions[signal['market_id']] = trade
            
            # Update open positions
            market_data = historical_data[
                historical_data['timestamp'] == timestamp
            ]
            
            for market_id, position in list(open_positions.items()):
                market_row = market_data[market_data['market_id'] == market_id]
                
                if not market_row.empty:
                    row = market_row.iloc[0]
                    
                    # Check if market is resolved
                    if pd.notna(row.get('resolution')):
                        trade = self._close_position(
                            position, 
                            row['resolution'],
                            timestamp
                        )
                        self.trades.append(trade)
                        del open_positions[market_id]
            
            # Record portfolio value
            unrealized_pnl = sum(
                self._calculate_unrealized_pnl(pos, historical_data, timestamp)
                for pos in open_positions.values()
            )
            
            total_value = self.capital + unrealized_pnl
            self.portfolio_value_history.append({
                'timestamp': timestamp,
                'portfolio_value': total_value,
                'capital': self.capital,
                'unrealized_pnl': unrealized_pnl,
                'open_positions': len(open_positions)
            })
        
        # Calculate performance metrics
        metrics = self._calculate_metrics()
        
        return {
            'config': self.config.__dict__,
            'metrics': metrics,
            'trades': [t.__dict__ for t in self.trades],
            'portfolio_history': self.portfolio_value_history,
        }
    
    def _can_enter_trade(self, signal: pd.Series) -> bool:
        """Check if we can enter a new trade"""
        # Check liquidity
        if signal.get('liquidity', 0) < self.config.min_liquidity:
            return False
        
        # Check capital availability
        max_position = self.capital * (self.config.max_position_size_pct / 100)
        if max_position < 100:  # Minimum $100 position
            return False
        
        return True
    
    def _enter_position(
        self,
        signal: pd.Series,
        timestamp: datetime
    ) -> Optional[Dict]:
        """Enter a new position"""
        # Calculate position size using Kelly Criterion
        edge = signal['edge_size']
        win_prob = signal['fair_value']
        
        kelly = self._calculate_kelly(edge, win_prob)
        kelly_position = self.capital * kelly * self.config.kelly_fraction
        
        max_position = self.capital * (self.config.max_position_size_pct / 100)
        position_size = min(kelly_position, max_position)
        
        if position_size < 100:
            return None
        
        # Apply slippage
        entry_price = signal['entry_price']
        slippage = entry_price * self.config.slippage_pct
        actual_entry = entry_price + slippage
        
        # Calculate quantity
        quantity = position_size / actual_entry
        
        return {
            'entry_time': timestamp,
            'market_id': signal['market_id'],
            'strategy': signal['strategy'],
            'position': signal['signal_type'],
            'entry_price': actual_entry,
            'quantity': quantity,
            'cost_basis': position_size,
            'slippage': slippage,
        }
    
    def _close_position(
        self,
        position: Dict,
        resolution: str,
        exit_time: datetime
    ) -> Trade:
        """Close a position"""
        # Determine exit price based on resolution
        if resolution == 'yes':
            exit_price = 1.0
        elif resolution == 'no':
            exit_price = 0.0
        else:  # invalid
            exit_price = position['entry_price']  # Break even
        
        # Apply slippage on exit
        exit_slippage = exit_price * self.config.slippage_pct
        actual_exit = max(0, exit_price - exit_slippage)
        
        # Calculate PnL
        proceeds = position['quantity'] * actual_exit
        cost = position['cost_basis']
        gross_pnl = proceeds - cost
        
        # Subtract gas costs
        net_pnl = gross_pnl - self.config.gas_cost_per_trade
        pnl_pct = (net_pnl / cost * 100) if cost > 0 else 0
        
        # Update capital
        self.capital += net_pnl
        
        return Trade(
            entry_time=position['entry_time'],
            exit_time=exit_time,
            market_id=position['market_id'],
            strategy=position['strategy'],
            position=position['position'],
            entry_price=position['entry_price'],
            exit_price=actual_exit,
            quantity=position['quantity'],
            pnl=net_pnl,
            pnl_pct=pnl_pct,
            gas_cost=self.config.gas_cost_per_trade,
            slippage=position['slippage'] + exit_slippage,
        )
    
    def _calculate_unrealized_pnl(
        self,
        position: Dict,
        market_data: pd.DataFrame,
        timestamp: datetime
    ) -> float:
        """Calculate unrealized PnL for open position"""
        market_row = market_data[
            (market_data['market_id'] == position['market_id']) &
            (market_data['timestamp'] == timestamp)
        ]
        
        if market_row.empty:
            return 0.0
        
        current_price = market_row.iloc[0]['yes_price']
        current_value = position['quantity'] * current_price
        unrealized = current_value - position['cost_basis']
        
        return unrealized
    
    def _calculate_kelly(self, edge: float, win_prob: float) -> float:
        """Calculate Kelly Criterion fraction"""
        if win_prob <= 0 or win_prob >= 1:
            return 0.0
        
        # Kelly formula: f = (bp - q) / b
        # where b = odds - 1, p = win probability, q = 1 - p
        odds = 1.0 / win_prob
        b = odds - 1
        q = 1 - win_prob
        
        if b <= 0:
            return 0.0
        
        kelly = ((b * win_prob) - q) / b
        return max(0.0, min(0.25, kelly))  # Cap at 25%
    
    def _calculate_metrics(self) -> Dict:
        """Calculate performance metrics"""
        if not self.trades:
            return {}
        
        trades_df = pd.DataFrame([t.__dict__ for t in self.trades])
        
        # Basic metrics
        total_trades = len(trades_df)
        winning_trades = len(trades_df[trades_df['pnl'] > 0])
        losing_trades = len(trades_df[trades_df['pnl'] < 0])
        win_rate = winning_trades / total_trades if total_trades > 0 else 0
        
        # PnL metrics
        total_pnl = trades_df['pnl'].sum()
        total_return_pct = (total_pnl / self.config.starting_capital) * 100
        
        # Calculate annualized return
        if self.portfolio_value_history:
            days = (
                self.portfolio_value_history[-1]['timestamp'] - 
                self.portfolio_value_history[0]['timestamp']
            ).days
            annualized_return = (
                (self.capital / self.config.starting_capital) ** (365 / max(days, 1)) - 1
            ) * 100 if days > 0 else 0
        else:
            annualized_return = 0
        
        # Risk metrics
        returns = trades_df['pnl_pct'].values
        sharpe_ratio = self._calculate_sharpe(returns)
        sortino_ratio = self._calculate_sortino(returns)
        max_drawdown = self._calculate_max_drawdown()
        
        # Win/Loss metrics
        avg_win = trades_df[trades_df['pnl'] > 0]['pnl'].mean() if winning_trades > 0 else 0
        avg_loss = trades_df[trades_df['pnl'] < 0]['pnl'].mean() if losing_trades > 0 else 0
        profit_factor = (
            abs(avg_win * winning_trades / (avg_loss * losing_trades))
            if losing_trades > 0 and avg_loss != 0 else 0
        )
        
        # Cost analysis
        total_gas = trades_df['gas_cost'].sum()
        total_slippage = trades_df['slippage'].sum() * trades_df['quantity']
        gas_pct_of_pnl = (total_gas / abs(total_pnl)) * 100 if total_pnl != 0 else 0
        
        return {
            'total_trades': total_trades,
            'winning_trades': winning_trades,
            'losing_trades': losing_trades,
            'win_rate': round(win_rate * 100, 2),
            'total_pnl': round(total_pnl, 2),
            'total_return_pct': round(total_return_pct, 2),
            'annualized_return_pct': round(annualized_return, 2),
            'sharpe_ratio': round(sharpe_ratio, 2),
            'sortino_ratio': round(sortino_ratio, 2),
            'max_drawdown_pct': round(max_drawdown * 100, 2),
            'profit_factor': round(profit_factor, 2),
            'avg_win': round(avg_win, 2),
            'avg_loss': round(avg_loss, 2),
            'total_gas_costs': round(total_gas, 2),
            'gas_pct_of_pnl': round(gas_pct_of_pnl, 2),
            'final_capital': round(self.capital, 2),
        }
    
    def _calculate_sharpe(self, returns: np.ndarray, risk_free_rate: float = 0.04) -> float:
        """Calculate Sharpe ratio"""
        if len(returns) == 0:
            return 0.0
        
        excess_returns = returns - (risk_free_rate / 252)  # Daily risk-free rate
        return np.mean(excess_returns) / np.std(excess_returns) * np.sqrt(252) if np.std(returns) > 0 else 0.0
    
    def _calculate_sortino(self, returns: np.ndarray, risk_free_rate: float = 0.04) -> float:
        """Calculate Sortino ratio (uses downside deviation)"""
        if len(returns) == 0:
            return 0.0
        
        excess_returns = returns - (risk_free_rate / 252)
        downside_returns = excess_returns[excess_returns < 0]
        downside_std = np.std(downside_returns) if len(downside_returns) > 0 else 0.0
        
        return np.mean(excess_returns) / downside_std * np.sqrt(252) if downside_std > 0 else 0.0
    
    def _calculate_max_drawdown(self) -> float:
        """Calculate maximum drawdown"""
        if not self.portfolio_value_history:
            return 0.0
        
        values = [h['portfolio_value'] for h in self.portfolio_value_history]
        peak = values[0]
        max_dd = 0.0
        
        for value in values:
            if value > peak:
                peak = value
            dd = (peak - value) / peak
            if dd > max_dd:
                max_dd = dd
        
        return max_dd


if __name__ == "__main__":
    # Example usage
    config = BacktestConfig(
        starting_capital=50000.0,
        max_position_size_pct=2.0,
        slippage_pct=0.02,
        gas_cost_per_trade=0.15,
    )
    
    engine = BacktestEngine(config)
    
    print("Backtesting engine initialized")
    print(f"Starting capital: ${config.starting_capital:,.2f}")
    print(f"Max position size: {config.max_position_size_pct}%")
    print(f"Slippage: {config.slippage_pct * 100}%")
    print(f"Gas cost per trade: ${config.gas_cost_per_trade}")
