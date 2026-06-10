.PHONY: build build-spa run test check lint fmt clean dev reset-db help

# ─── Default ──────────────────────────────────────
help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-15s\033[0m %s\n", $$1, $$2}'

# ─── Building ────────────────────────────────────
build: ## Build all crates (debug)
	cargo build

release: ## Build all crates (release, optimized)
	cargo build --release

build-spa: ## Build the SPA and embed it into a single release binary (serves /api + UI same-origin)
	npm --prefix frontend ci
	npm --prefix frontend run build
	cargo build -p flexpm-api --release --features embed-spa
	@echo ""
	@echo "✓ Single binary: target/release/flexpm-api"
	@echo "  Serves the API at /api/* and the SPA same-origin from one process."

# ─── Running ─────────────────────────────────────
dev: frontend/node_modules ## Start API + frontend dev server — Ctrl-C stops both
	@trap 'kill 0' SIGINT; \
	cargo run --bin flexpm-api & \
	npm --prefix frontend run dev & \
	wait

frontend/node_modules:
	npm --prefix frontend install

run: ## Run the release binary (build-spa first)
	./target/release/flexpm-api

debug: ## Start API with verbose logging (no frontend)
	RUST_LOG=flexpm_api=debug,flexpm_db=debug,tower_http=debug cargo run --bin flexpm-api

cli: ## Run the CLI (use ARGS="..." to pass arguments)
	cargo run --bin flexpm-cli -- $(ARGS)

# ─── Testing ─────────────────────────────────────
test: ## Run all tests
	cargo test

test-verbose: ## Run all tests with output
	cargo test -- --nocapture

test-core: ## Run only core unit tests
	cargo test -p flexpm-core

test-db: ## Run only database integration tests
	cargo test -p flexpm-db

# ─── Code Quality ────────────────────────────────
check: ## Type-check without building
	cargo check --workspace

lint: ## Run clippy linter
	cargo clippy --workspace -- -D warnings

fmt: ## Format all code
	cargo fmt --all

fmt-check: ## Check formatting (for CI)
	cargo fmt --all -- --check

# ─── Database ────────────────────────────────────
reset-db: ## Delete the database (re-created on next run)
	rm -f flexpm.db flexpm.db-shm flexpm.db-wal
	@echo "Database deleted. Run 'make run' to re-create."

inspect-db: ## Open the database in SQLite CLI
	sqlite3 flexpm.db

# ─── Quick API Tests ─────────────────────────────
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
