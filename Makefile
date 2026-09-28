.PHONY: demo install dev test rust-test diagnostic reset-lancedb clean-dev-cache clean-all-generated clean-dev-cache-dry-run capture-baseline capture-baseline-verify qa-seed qa-retrieval vault-health phase-progress gitlab-plan gitlab-sync

CARGO_BUILD_JOBS ?= 1
PYTHON ?= python3
QA_PROFILE ?= $(HOME)/Library/Application Support/com.fndr.app.qa
QA_CORPUS ?= $(CURDIR)/scripts/demo/knowledge-worker-week.json
QA_QUERIES ?= $(CURDIR)/scripts/demo/knowledge-worker-queries.json
QA_EVIDENCE_DIR ?= $(CURDIR)/docs/evidence/W02
QA_RETRIEVAL_MD ?= $(QA_EVIDENCE_DIR)/retrieval-baseline-seeded.md
QA_RETRIEVAL_JSON ?= $(QA_EVIDENCE_DIR)/retrieval-baseline-seeded.json
VAULT_HEALTH_OUT ?= $(QA_EVIDENCE_DIR)/vault-health-owner.md

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

qa-seed:
	FNDR_DEMO_DIR="$(QA_PROFILE)" FNDR_DEMO_CORPUS="$(QA_CORPUS)" CARGO_BUILD_JOBS="$(CARGO_BUILD_JOBS)" ./scripts/demo/seed-demo-profile.sh --reset

qa-retrieval:
	mkdir -p "$(QA_EVIDENCE_DIR)"
	cd src-tauri && CARGO_BUILD_JOBS="$(CARGO_BUILD_JOBS)" cargo run --example retrieval_qa -- --data-dir "$(QA_PROFILE)" --cases "$(QA_QUERIES)" --out "$(QA_RETRIEVAL_MD)" --json "$(QA_RETRIEVAL_JSON)"

vault-health:
	mkdir -p "$(QA_EVIDENCE_DIR)"
	$(PYTHON) scripts/audit/vault_health.py $(if $(DB),--db "$(DB)") --out "$(or $(OUT),$(VAULT_HEALTH_OUT))"

phase-progress:
	python3 scripts/team/phase_progress.py --manifest docs/superpowers/plans/2026-09-21-beta-final-master-plan.md --api

gitlab-plan:
	python3 scripts/team/gitlab_sync.py plan

gitlab-sync:
	python3 scripts/team/gitlab_sync.py sync $(if $(APPLY),--apply) $(if $(UPDATE),--update)
