.PHONY: help build test lint fmt check hooks run-server run-client clean release-dry

.DEFAULT_GOAL := help

BLUE  := \033[0;34m
GREEN := \033[0;32m
CYAN  := \033[0;36m
NC    := \033[0m

help: ## Show targets
	@echo "$(BLUE)════════════════════════════════════════$(NC)"
	@echo "$(BLUE)           掼蛋 Guandan — Make          $(NC)"
	@echo "$(BLUE)════════════════════════════════════════$(NC)"
	@grep -E '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  $(CYAN)%-14s$(NC) %s\n", $$1, $$2}'

hooks: ## Install git hooks (fmt + clippy + check)
	@git config core.hooksPath .githooks
	@chmod +x .githooks/pre-commit
	@echo "$(GREEN)✓ hooks installed (core.hooksPath=.githooks)$(NC)"

fmt: ## Format all crates
	cargo fmt --all

lint: ## Clippy with -D warnings
	cargo clippy --workspace --all-targets -- -D warnings

check: ## Type-check workspace
	cargo check --workspace --all-targets

test: ## Run all tests
	cargo test --workspace

build: ## Release build
	cargo build --workspace --release

run-server: ## Start server on :9100
	cargo run -p guandan-server -- --bind 0.0.0.0:9100

run-client: ## Start TUI client
	cargo run -p guandan-client -- --server ws://127.0.0.1:9100

clean: ## Remove target/
	cargo clean

release-dry: ## Show how to cut a release
	@echo "1. Ensure CI is green on main"
	@echo "2. git tag v0.1.0 && git push origin v0.1.0"
	@echo "3. GitHub Actions Release builds multi-platform binaries"
