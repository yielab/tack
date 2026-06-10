.PHONY: build run dev debug cli test test-verbose test-core test-db check lint fmt fmt-check reset-db inspect-db api-health api-stats api-projects clean clean-all help

# ─── Default ──────────────────────────────────────
help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-15s\033[0m %s\n", $$1, $$2}'

# ─── Building & Running ───────────────────────────
build: ## Build the full app — frontend + release binary with embedded UI
	npm --prefix frontend ci
	npm --prefix frontend run build
	cargo build -p flexpm-api --release --features embed-spa
	@echo ""
	@echo "  Binary: target/release/flexpm-api"
	@echo "  Run:    ./target/release/flexpm-api"
	@echo "  Open:   http://127.0.0.1:3210"

run: build ## Build and start the full app (API + UI, single process)
	./target/release/flexpm-api

dev: frontend/node_modules ## Development mode: API + Vite hot-reload (Ctrl-C stops both)
	@trap 'kill 0' SIGINT; \
	cargo run --bin flexpm-api & \
	npm --prefix frontend run dev & \
	wait

frontend/node_modules:
	npm --prefix frontend install

debug: ## Start API only with verbose logging
	RUST_LOG=flexpm_api=debug,flexpm_db=debug,tower_http=debug cargo run --bin flexpm-api

cli: ## Run the CLI (use ARGS="..." to pass arguments)
	cargo run --bin flexpm-cli -- $(ARGS)

# ─── Testing ─────────────────────────────────────
test: ## Run all tests
	cargo test --workspace

test-verbose: ## Run all tests with output
	cargo test --workspace -- --nocapture

test-core: ## Run only core unit tests
	cargo test -p flexpm-core

test-db: ## Run only database integration tests
	cargo test -p flexpm-db

# ─── Code Quality ────────────────────────────────
check: ## Type-check without building
	cargo check --workspace

lint: ## Run clippy linter
	cargo clippy --workspace -- -D warnings

fmt: ## Format all Rust code
	cargo fmt --all

fmt-check: ## Check formatting (used in CI)
	cargo fmt --all -- --check

# ─── Database ────────────────────────────────────
reset-db: ## Delete the database (auto-recreated on next run)
	rm -f flexpm.db flexpm.db-shm flexpm.db-wal
	@echo "Database deleted. Run 'make run' or 'make dev' to recreate."

inspect-db: ## Open the live database in SQLite CLI
	sqlite3 flexpm.db

# ─── Quick API Checks ────────────────────────────
api-health: ## Check server health
	@curl -s http://localhost:3210/api/health | python3 -m json.tool 2>/dev/null || curl -s http://localhost:3210/api/health

api-stats: ## Show database statistics
	@curl -s http://localhost:3210/api/debug/db-stats | python3 -m json.tool 2>/dev/null || curl -s http://localhost:3210/api/debug/db-stats

api-projects: ## List all projects
	@curl -s http://localhost:3210/api/projects | python3 -m json.tool 2>/dev/null || curl -s http://localhost:3210/api/projects

# ─── Cleanup ─────────────────────────────────────
clean: ## Remove build artifacts
	cargo clean

clean-all: clean reset-db ## Remove build artifacts and database
