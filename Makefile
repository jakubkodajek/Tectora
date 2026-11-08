# Single entry point for humans, agents and CI. `make check` is the gate an
# agent runs (and shows the output of) before it calls a change done.
.PHONY: help setup lint test e2e sweeps build check dev preview agents

# The Tauri crate needs WebKitGTK on Linux; leave it out where it is missing
# (cloud containers). macOS and Windows always build it.
CARGO_EXCLUDE := $(shell if [ "$$(uname -s)" = "Linux" ] && ! pkg-config --exists webkit2gtk-4.1 2>/dev/null; then echo "--exclude tectora-app"; fi)

help: ## List the targets
	@grep -E '^[a-z-]+:.*## ' $(MAKEFILE_LIST) | awk -F':.*## ' '{printf "%-8s %s\n", $$1, $$2}'

setup: ## Install UI dependencies
	cd app && npm ci

lint: ## rustfmt check, clippy, svelte-check
	cargo fmt --all --check
	cargo clippy --workspace --all-targets $(CARGO_EXCLUDE) -- -D warnings
	cd app && npm run check

test: ## Rust tests (core fixtures, analytic, properties, schema) and UI unit tests
	cargo test --workspace $(CARGO_EXCLUDE)
	cd app && npm test

e2e: ## UI end-to-end tests, once app/e2e exists
	@if [ -d app/e2e ]; then cd app && npx playwright test --trace off; else echo "no app/e2e suite yet"; fi

sweeps: ## i18n and colour-contrast sweeps
	python3 .claude/skills/tectora-i18n/sweep.py
	python3 .claude/skills/tectora-design/contrast.py

build: ## UI production build
	cd app && npm run build

check: agents lint test build ## Everything CI checks (run `make sweeps` too for UI changes)

dev: ## Run the desktop app
	cd app && npx tauri dev

preview: ## UI in a browser on the real core, http://localhost:1430
	cd app && npm run preview

agents: ## Agent setup: generated Codex/Copilot copies in sync, hooks behave
	@python3 scripts/sync_agents.py --check
	@bash scripts/test_hooks.sh
