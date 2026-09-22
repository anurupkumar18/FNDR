.PHONY: demo install dev test rust-test diagnostic reset-lancedb clean-dev-cache clean-all-generated clean-dev-cache-dry-run capture-baseline capture-baseline-verify phase-progress

install:
	npm install

demo: install
	npm run tauri dev

test:
	npm run typecheck
	npm test
	npm run build
	cd src-tauri && cargo test

rust-test:
	cd src-tauri && cargo fmt --check && cargo clippy --all-targets && cargo test

diagnostic:
	./scripts/run_embedding_search_diagnostic.sh

reset-lancedb:
	./scripts/reset_lancedb.sh

clean-dev-cache:
	./scripts/clean-dev-build-cache.sh --yes

clean-all-generated:
	./scripts/clean-dev-build-cache.sh --yes --all

clean-dev-cache-dry-run:
	./scripts/clean-dev-build-cache.sh --dry-run

capture-baseline:
	@test -n "$(METRICS)" || (echo "usage: make capture-baseline METRICS=/path/m.ndjson OUT=docs/evidence/W01/capture-baseline.md" && exit 1)
	@test -n "$(OUT)" || (echo "usage: make capture-baseline METRICS=/path/m.ndjson OUT=docs/evidence/W01/capture-baseline.md" && exit 1)
	@machine='$(MACHINE)'; if [ -z "$$machine" ]; then machine="$$(sysctl -n machdep.cpu.brand_string), $$(( $$(sysctl -n hw.memsize) / 1073741824 )) GB"; fi; python3 scripts/bench/summarize_metrics.py "$(METRICS)" --out "$(OUT)" --machine "$$machine" --build "$(or $(BUILD),release)" $(if $(BEFORE_CONTEXT_P95),--before-context-p95 "$(BEFORE_CONTEXT_P95)")

capture-baseline-verify:
	@test -n "$(METRICS)" || (echo "usage: make capture-baseline-verify METRICS=/path/m.ndjson" && exit 1)
	python3 scripts/bench/validate_capture_baseline.py "$(METRICS)"

phase-progress:
	python3 scripts/team/phase_progress.py --manifest docs/superpowers/plans/2026-09-21-beta-final-master-plan.md --api
