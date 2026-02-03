.PHONY: help build run test clean docker-build docker-up docker-down db-migrate

help:
	@echo "Polymarket Trading Bot - Make Commands"
	@echo ""
	@echo "  make build          - Build Rust binary"
	@echo "  make run            - Run trading bot locally"
	@echo "  make test           - Run all tests"
	@echo "  make clean          - Clean build artifacts"
	@echo "  make docker-build   - Build Docker images"
	@echo "  make docker-up      - Start all services"
	@echo "  make docker-down    - Stop all services"
	@echo "  make db-migrate     - Run database migrations"
	@echo "  make backtest       - Run backtesting"

build:
	cargo build --release

run:
	cargo run --release

test:
	cargo test
	cd python && pytest

clean:
	cargo clean
	find . -type d -name __pycache__ -exec rm -rf {} +

docker-build:
	cd docker && docker-compose build

docker-up:
	cd docker && docker-compose up -d

docker-down:
	cd docker && docker-compose down

docker-logs:
	cd docker && docker-compose logs -f

db-migrate:
	docker-compose exec postgres psql -U trading_bot -d polymarket_bot -f /docker-entrypoint-initdb.d/01-schema.sql

backtest:
	cargo run --bin backtest -- --config config/default.yaml

install-rust-deps:
	rustup update
	cargo install sqlx-cli

install-python-deps:
	cd python && pip install -r requirements.txt

setup-dev: install-rust-deps install-python-deps
	cp .env.example .env
	@echo "Please edit .env with your API keys"
