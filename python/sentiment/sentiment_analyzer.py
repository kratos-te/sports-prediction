"""
Sentiment Analysis Module for Sports News and Social Media

Strategy 5: Sentiment-Implied Probability Gap
Edge: Social media sentiment often overreacts
"""

import torch
from transformers import AutoTokenizer, AutoModelForSequenceClassification
from vaderSentiment.vaderSentiment import SentimentIntensityAnalyzer
from typing import Dict, List, Tuple
import re


class SentimentAnalyzer:
    """
    Multi-model sentiment analyzer combining VADER and BERT
    for sports-specific sentiment analysis
    """

    def __init__(self, use_bert: bool = True):
        """
        Initialize sentiment analyzers
        
        Args:
            use_bert: Whether to use BERT model (slower but more accurate)
        """
        # VADER for quick sentiment analysis
        self.vader = SentimentIntensityAnalyzer()
        
        # BERT for deep sentiment analysis
        self.use_bert = use_bert
        if use_bert:
            model_name = "cardiffnlp/twitter-roberta-base-sentiment"
            self.tokenizer = AutoTokenizer.from_pretrained(model_name)
            self.model = AutoModelForSequenceClassification.from_pretrained(model_name)
            self.model.eval()
    
    def analyze_text(self, text: str) -> Dict[str, float]:
        """
        Analyze sentiment of a single text
        
        Args:
            text: Input text to analyze
            
        Returns:
            Dictionary with sentiment scores
        """
        # Clean text
        cleaned_text = self._clean_text(text)
        
        # VADER sentiment
        vader_scores = self.vader.polarity_scores(cleaned_text)
        
        result = {
            'vader_positive': vader_scores['pos'],
            'vader_negative': vader_scores['neg'],
            'vader_neutral': vader_scores['neu'],
            'vader_compound': vader_scores['compound'],
        }
        
        # BERT sentiment (if enabled)
        if self.use_bert:
            bert_scores = self._bert_sentiment(cleaned_text)
            result.update({
                'bert_positive': bert_scores[2],
                'bert_neutral': bert_scores[1],
                'bert_negative': bert_scores[0],
            })
            
            # Combined sentiment (weighted average)
            result['combined_positive'] = (
                0.4 * result['vader_positive'] + 
                0.6 * result['bert_positive']
            )
            result['combined_negative'] = (
                0.4 * result['vader_negative'] + 
                0.6 * result['bert_negative']
            )
        else:
            result['combined_positive'] = result['vader_positive']
            result['combined_negative'] = result['vader_negative']
        
        # Calculate net sentiment
        result['net_sentiment'] = (
            result['combined_positive'] - result['combined_negative']
        )
        
        return result
    
    def analyze_batch(self, texts: List[str]) -> List[Dict[str, float]]:
        """
        Analyze sentiment of multiple texts
        
        Args:
            texts: List of texts to analyze
            
        Returns:
            List of sentiment dictionaries
        """
        return [self.analyze_text(text) for text in texts]
    
    def aggregate_sentiment(
        self, 
        texts: List[str],
        weights: List[float] = None
    ) -> Dict[str, float]:
        """
        Aggregate sentiment across multiple texts
        
        Args:
            texts: List of texts
            weights: Optional weights for each text
            
        Returns:
            Aggregated sentiment scores
        """
        if not texts:
            return {'net_sentiment': 0.0, 'confidence': 0.0}
        
        sentiments = self.analyze_batch(texts)
        
        if weights is None:
            weights = [1.0] * len(texts)
        
        total_weight = sum(weights)
        
        # Weighted average
        avg_positive = sum(
            s['combined_positive'] * w 
            for s, w in zip(sentiments, weights)
        ) / total_weight
        
        avg_negative = sum(
            s['combined_negative'] * w 
            for s, w in zip(sentiments, weights)
        ) / total_weight
        
        net_sentiment = avg_positive - avg_negative
        
        # Confidence based on agreement between sources
        sentiments_list = [s['net_sentiment'] for s in sentiments]
        std_dev = self._std_dev(sentiments_list)
        confidence = max(0.0, 1.0 - std_dev)
        
        return {
            'avg_positive': avg_positive,
            'avg_negative': avg_negative,
            'net_sentiment': net_sentiment,
            'confidence': confidence,
            'sample_size': len(texts),
        }
    
    def sentiment_to_probability(
        self,
        sentiment_score: float,
        baseline_prob: float = 0.5
    ) -> float:
        """
        Convert sentiment score to implied probability
        
        Args:
            sentiment_score: Net sentiment (-1 to 1)
            baseline_prob: Starting probability
            
        Returns:
            Sentiment-implied probability
        """
        # Sentiment adjustment (scaled to +/- 20%)
        adjustment = sentiment_score * 0.2
        
        # Apply adjustment to baseline
        implied_prob = baseline_prob + adjustment
        
        # Clamp to valid range
        return max(0.05, min(0.95, implied_prob))
    
    def _clean_text(self, text: str) -> str:
        """Clean and normalize text"""
        # Remove URLs
        text = re.sub(r'http\S+|www\S+|https\S+', '', text, flags=re.MULTILINE)
        
        # Remove mentions and hashtags (but keep the text)
        text = re.sub(r'@\w+', '', text)
        text = re.sub(r'#(\w+)', r'\1', text)
        
        # Remove extra whitespace
        text = ' '.join(text.split())
        
        return text
    
    def _bert_sentiment(self, text: str) -> Tuple[float, float, float]:
        """
        Get BERT sentiment scores
        
        Returns:
            (negative, neutral, positive) probabilities
        """
        inputs = self.tokenizer(
            text,
            return_tensors="pt",
            truncation=True,
            max_length=512,
            padding=True
        )
        
        with torch.no_grad():
            outputs = self.model(**inputs)
            scores = outputs.logits.softmax(dim=1)[0].tolist()
        
        return tuple(scores)
    
    @staticmethod
    def _std_dev(values: List[float]) -> float:
        """Calculate standard deviation"""
        if len(values) <= 1:
            return 0.0
        
        mean = sum(values) / len(values)
        variance = sum((x - mean) ** 2 for x in values) / len(values)
        return variance ** 0.5


class SportsNewsClassifier:
    """
    Classify news importance for sports events
    
    Strategy 3: Injury News Scalping
    """
    
    KEYWORDS = {
        'injury': ['injured', 'injury', 'hurt', 'questionable', 'doubtful', 'out'],
        'lineup': ['starting', 'lineup', 'benched', 'scratched'],
        'performance': ['mvp', 'career-high', 'record', 'milestone'],
        'roster': ['traded', 'signed', 'released', 'waived'],
    }
    
    PLAYER_IMPORTANCE = {
        'star': 3.0,      # Superstar players
        'starter': 2.0,   # Regular starters
        'rotation': 1.0,  # Rotation players
        'bench': 0.3,     # Bench players
    }
    
    def __init__(self):
        self.sentiment_analyzer = SentimentAnalyzer(use_bert=False)
    
    def classify_news_importance(
        self,
        news_text: str,
        player_name: str = None,
        player_importance: str = 'rotation'
    ) -> Dict[str, any]:
        """
        Classify news importance and expected impact
        
        Args:
            news_text: News article or tweet text
            player_name: Player mentioned (if known)
            player_importance: Player tier (star/starter/rotation/bench)
            
        Returns:
            Classification results with impact score
        """
        text_lower = news_text.lower()
        
        # Detect news category
        category = 'other'
        for cat, keywords in self.KEYWORDS.items():
            if any(kw in text_lower for kw in keywords):
                category = cat
                break
        
        # Calculate importance multiplier
        importance_mult = self.PLAYER_IMPORTANCE.get(player_importance, 1.0)
        
        # Base impact by category
        category_impact = {
            'injury': 0.8,
            'lineup': 0.6,
            'roster': 0.7,
            'performance': 0.4,
            'other': 0.2,
        }
        
        base_impact = category_impact.get(category, 0.2)
        
        # Calculate final impact score
        impact_score = base_impact * importance_mult
        
        # Get sentiment
        sentiment = self.sentiment_analyzer.analyze_text(news_text)
        
        # Urgency (how quickly market should react)
        urgency = 'high' if category in ['injury', 'lineup'] else 'medium'
        
        return {
            'category': category,
            'player_name': player_name,
            'player_importance': player_importance,
            'impact_score': min(1.0, impact_score),
            'sentiment': sentiment['net_sentiment'],
            'urgency': urgency,
            'expected_price_impact': impact_score * 0.1,  # Expected % price move
        }


if __name__ == "__main__":
    # Example usage
    analyzer = SentimentAnalyzer(use_bert=True)
    
    # Test sentiment analysis
    texts = [
        "LeBron James is playing amazing tonight! Lakers looking unstoppable!",
        "Patrick Mahomes injured, questionable for Sunday's game",
        "The team is struggling badly, worst performance of the season",
    ]
    
    for text in texts:
        result = analyzer.analyze_text(text)
        print(f"\nText: {text}")
        print(f"Sentiment: {result['net_sentiment']:.3f}")
        print(f"Implied Prob: {analyzer.sentiment_to_probability(result['net_sentiment']):.3f}")
    
    # Test news classification
    classifier = SportsNewsClassifier()
    news = "BREAKING: Patrick Mahomes (ankle) ruled OUT for Sunday vs Chargers"
    result = classifier.classify_news_importance(
        news,
        player_name="Patrick Mahomes",
        player_importance="star"
    )
    print(f"\nNews Classification: {result}")
