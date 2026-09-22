.PHONY: demo install dev test rust-test diagnostic reset-lancedb clean-dev-cache clean-all-generated clean-dev-cache-dry-run capture-baseline

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
	python3 scripts/bench/summarize_metrics.py "$(METRICS)" --out "$(OUT)" --machine "$$(sysctl -n machdep.cpu.brand_string), $$(( $$(sysctl -n hw.memsize) / 1073741824 )) GB" --build release
