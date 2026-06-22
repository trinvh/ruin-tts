# Beesoft Studio — common commands. Run `make` (or `make help`) to list targets.
.DEFAULT_GOAL := help
.PHONY: help dev dev-bundled build sidecars test fmt clippy clean upload-models

# Host target triple (e.g. aarch64-apple-darwin) for staging bundled sidecars.
TRIPLE := $(shell rustc -vV | sed -n 's/host: //p')
SIDECARS := vieneu-server studio-server media-ai
MAIN_CRATES := -p vieneu-core -p vieneu-server -p vieneu-cli -p studio -p media-ai

dev: ## Run the app with FULL hot-reload (UI via Vite + Rust sidecars via cargo-watch)
	bash scripts/dev.sh

dev-bundled: sidecars ## Run the app the way it ships (prebuilt sidecars, no Rust hot-reload)
	@# Kill any sidecars left over from a previous run so launching never clashes.
	@-pkill -f 'release/vieneu-server' 2>/dev/null; pkill -f 'release/studio-server' 2>/dev/null; pkill -f 'release/media-ai' 2>/dev/null; true
	pnpm -C ui install
	pnpm -C ui tauri dev

build: sidecars ## Build the self-contained desktop app (single bundled installer)
	pnpm -C ui install
	@mkdir -p ui/src-tauri/binaries
	@for b in $(SIDECARS); do cp "target/release/$$b" "ui/src-tauri/binaries/$$b-$(TRIPLE)"; done
	pnpm -C ui tauri build
	@echo "→ installer in ui/src-tauri/target/release/bundle/"

sidecars: ## Build the release sidecar binaries the app bundles + launches
	cargo build --release -p vieneu-server -p studio -p media-ai

test: ## Run the workspace test suite
	cargo test --workspace

fmt: ## Format the Rust crates
	cargo fmt $(MAIN_CRATES)

clippy: ## Lint the Rust crates (warnings as errors)
	cargo clippy $(MAIN_CRATES) -- -D warnings

upload-models: ## Export + upload the diarization ONNX models to HF (run once)
	bash tools/upload-models.sh

clean: ## Remove build artifacts
	cargo clean
	rm -rf ui/dist ui/node_modules ui/src-tauri/binaries

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) \
		| awk 'BEGIN{FS=":.*?## "}{printf "  \033[36m%-14s\033[0m %s\n", $$1, $$2}'
