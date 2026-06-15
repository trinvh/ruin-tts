# VieNeu-TTS (Rust) — common commands.
# Run `make` or `make help` to list targets.

ADDR    ?= 127.0.0.1:8080
WORKERS ?= 2
VOICE   ?= Bình An
TEXT    ?= Xin chào, đây là bản dựng tiếng Việt bằng Rust.
FORMAT  ?= wav
OUT     ?= out.$(FORMAT)
SERVER  := ./target/release/vieneu-server
CLI     := ./target/release/vieneu

.DEFAULT_GOAL := help
.PHONY: help build build-debug test fmt fmt-check clippy clean \
        server server-dev cli voices synth batch smoke \
        ui-install ui-build ui-dev ui-web e2e \
        studio-server studio-web studio-test

## ── Studio (webnovel → audiobook → YouTube automation) ─────────────
studio-server: ## Run the studio automation server (set RUIN_API_KEY)
	cargo run -p studio --bin studio-server -- --addr 127.0.0.1:8090

studio-web: ## Run the studio operator UI (React Flow) dev server
	pnpm -C studio/web install && pnpm -C studio/web dev

studio-test: ## Run studio crate tests
	cargo test -p studio

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) \
		| awk 'BEGIN{FS=":.*?## "}{printf "  \033[36m%-14s\033[0m %s\n", $$1, $$2}'

## ── Build & quality ────────────────────────────────────────────────
build: ## Release build of all crates (core, server, cli)
	cargo build --workspace --release

build-debug: ## Debug build of all crates
	cargo build --workspace

test: ## Run the full test suite
	cargo test --workspace

fmt: ## Format the Rust crates (skips the vendored sea-g2p-rs)
	cargo fmt -p vieneu-core -p vieneu-server -p vieneu-cli

fmt-check: ## Check formatting without writing
	cargo fmt -p vieneu-core -p vieneu-server -p vieneu-cli -- --check

clippy: ## Lint our crates with warnings as errors
	cargo clippy -p vieneu-core -p vieneu-server -p vieneu-cli -- -D warnings

clean: ## Remove build artifacts
	cargo clean
	rm -rf ui/dist ui/node_modules

## ── Run: server & CLI ──────────────────────────────────────────────
server: build ## Run the HTTP API server ($(ADDR), $(WORKERS) workers)
	$(SERVER) --addr $(ADDR) --workers $(WORKERS)

server-dev: ## Run the API via cargo with debug logs (easy restart)
	RUST_LOG=vieneu_server=debug,ort=warn cargo run -p vieneu-server -- --addr $(ADDR) --workers $(WORKERS)

voices: build ## List built-in preset voices
	$(CLI) voices

synth: build ## Synthesize TEXT → OUT (VOICE, FORMAT overridable)
	$(CLI) synth --text "$(TEXT)" --voice "$(VOICE)" --format $(FORMAT) --out $(OUT)
	@echo "wrote $(OUT)"

batch: build ## Batch: chapters/*.txt → audio/*.$(FORMAT) (set IN/OUTDIR)
	$(CLI) batch --input-dir $(or $(IN),chapters) --out-dir $(or $(OUTDIR),audio) \
		--voice "$(VOICE)" --format $(FORMAT) $(if $(WORKERS),--workers $(WORKERS),)

smoke: ## Greedy single-clip smoke test (writes /tmp/vieneu_smoke.wav)
	cargo run -p vieneu-core --example smoke --release -- "$(TEXT)" /tmp/vieneu_smoke.wav "$(VOICE)"

## ── Tauri desktop app ──────────────────────────────────────────────
ui-install: ## Install UI dependencies (pnpm)
	pnpm -C ui install

ui-build: build ui-install ## Production-build the Tauri app bundle
	pnpm -C ui tauri build

ui-dev: build ui-install ## Run the Tauri app in dev mode (spawns the server)
	pnpm -C ui tauri dev

ui-web: ui-install ## Frontend-only dev server (browser) → calls API on $(ADDR)
	pnpm -C ui dev

## ── End-to-end smoke of the live API ───────────────────────────────
e2e: build ## Boot the server and exercise every endpoint, then shut down
	@echo "── starting server on $(ADDR) ──"
	@$(SERVER) --addr $(ADDR) --workers 1 >/tmp/vieneu_e2e.log 2>&1 & echo $$! >/tmp/vieneu_e2e.pid; \
	trap 'kill $$(cat /tmp/vieneu_e2e.pid) 2>/dev/null' EXIT; \
	for i in $$(seq 1 90); do curl -fs http://$(ADDR)/health >/dev/null 2>&1 && break; sleep 1; done; \
	echo "── /v1/info ──";  curl -fs http://$(ADDR)/v1/info; echo; \
	echo "── /v1/voices ──"; curl -fs http://$(ADDR)/v1/voices | head -c 200; echo; \
	echo "── /v1/tts (wav) ──"; \
	  curl -fs -X POST http://$(ADDR)/v1/tts -H 'content-type: application/json' \
	    -d '{"text":"Kiểm tra đầu cuối.","voice":"$(VOICE)","temperature":0.0}' \
	    -o /tmp/e2e.wav -w 'http %{http_code} %{size_download} bytes\n'; \
	echo "── /v1/tts (mp3) ──"; \
	  curl -fs -X POST http://$(ADDR)/v1/tts -H 'content-type: application/json' \
	    -d '{"text":"Xuất MP3.","voice":"$(VOICE)","format":"mp3","temperature":0.0}' \
	    -o /tmp/e2e.mp3 -w 'http %{http_code} %{size_download} bytes\n'; \
	echo "── async job ──"; \
	  JOB=$$(curl -fs -X POST http://$(ADDR)/v1/jobs -H 'content-type: application/json' \
	    -d '{"text":"Công việc bất đồng bộ dài hơn một chút.","format":"mp3"}' \
	    | sed -n 's/.*"job_id":"\([^"]*\)".*/\1/p'); \
	  echo "job_id=$$JOB"; \
	  for i in $$(seq 1 60); do curl -fs http://$(ADDR)/v1/jobs/$$JOB | grep -q '"ready":true' && break; sleep 1; done; \
	  curl -fs http://$(ADDR)/v1/jobs/$$JOB/download -o /tmp/e2e_job.mp3 -w 'job download http %{http_code} %{size_download} bytes\n'; \
	echo "── results ──"; file /tmp/e2e.wav /tmp/e2e.mp3 /tmp/e2e_job.mp3; \
	echo "e2e OK"
