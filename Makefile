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

test: scripts-test
	npm run typecheck
	npm test
	npm run build
	cd src-tauri && cargo test

# The Python tests for the audit, team, model, bench and bootstrap scripts.
# They take a few seconds and nothing else runs them.
.PHONY: scripts-test
scripts-test:
	@for dir in $$(find scripts -name "test_*.py" -exec dirname {} \; | sort -u); do \
		$(PYTHON) -m unittest discover -q -s "$$dir" -t "$$dir" -p "test_*.py" || exit 1; \
	done

# Runs `cargo test --lib` on the committed tree plus the files named in FILES,
# in a scratch copy, so another session's uncommitted work cannot break it.
#   make test-clean FILES="src-tauri/src/a.rs src-tauri/src/b.rs" FILTER=context_runtime
.PHONY: test-clean
test-clean:
	./scripts/dev/test-clean.sh $(foreach file,$(FILES),-f "$(file)") $(FILTER)

# Linked test and example executables pile up in the shared build cache, a
# few hundred MB each, one set per source path that was ever built (this
# checkout, the test-clean copy). Removes those older than a day; a stale one
# is relinked in about a minute if it is ever needed. On 2026-10-09 this freed
# 32 GB of a 67 GB cache.
.PHONY: clean-test-binaries
CARGO_SHARED_TARGET ?= $(HOME)/.cache/cargo-target-shared
clean-test-binaries:
	@find "$(CARGO_SHARED_TARGET)/debug/deps" "$(CARGO_SHARED_TARGET)/debug/examples" -maxdepth 1 -type f -perm +111 -size +100M -mtime +1 -delete 2>/dev/null; \
	df -h "$(HOME)" | tail -1 | awk '{print $$4 " free"}'

# The owner-vault quality gate: copies the vault, scores the copy
# (examples/vault_qa.rs) and checks the scorecard against
# scripts/audit/vault-quality-thresholds.json. Counts only; the real vault is
# never opened for writing and the copy is removed afterwards.
.PHONY: qa-vault
VAULT_PROFILE ?= $(HOME)/Library/Application Support/com.fndr.app
qa-vault:
	@copy="$$(mktemp -d)"; trap 'rm -rf "$$copy"' EXIT; \
	cp -R "$(VAULT_PROFILE)/lancedb" "$$copy/lancedb" && \
	(cd src-tauri && CARGO_BUILD_JOBS="$(CARGO_BUILD_JOBS)" cargo run -q --example vault_qa -- --data-dir "$$copy" --sample 40 > "$$copy/scorecard.json") && \
	$(PYTHON) scripts/audit/vault_quality_check.py --scorecard "$$copy/scorecard.json"

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
	date +%F > "$(QA_PROFILE)/.seeded-on"

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

# VS-66: rebuild the Beta demo recall chart (CSV and PNG) from committed
# evidence: the VS-03 baselines and the accepted references. The PNG needs
# matplotlib.
.PHONY: recall-chart
recall-chart:
	$(PYTHON) scripts/audit/recall_chart.py

# VS-04: retrieval merge gate. Reseeds the QA profile (QA_SKIP_SEED=1 skips it),
# reruns retrieval_qa into a scratch report, and compares it with the accepted
# reference for the case set. Fails on a Recall@5 drop over 0.05 on any path or
# on any query that a path found in its top ten and now misses.
.PHONY: qa-retrieval-check
QA_CASE_SET ?= $(patsubst %-queries.json,%,$(notdir $(QA_QUERIES)))
QA_REFERENCE ?= $(CURDIR)/scripts/demo/retrieval-reference/$(QA_CASE_SET).json
QA_CHECK_DIR ?= $(CURDIR)/src-tauri/target/qa-retrieval-check/$(QA_CASE_SET)$(if $(QA_CHUNKS),-chunks,)
# VS-18: QA_CHUNKS=1 indexes the evaluation copy with BGE chunk rows and turns
# the chunk route on; the check against the usual reference then shows the
# chunk-on deltas. Needs the BGE model (scripts/bootstrap/download-embedding-model.sh).
QA_CHUNK_ARGS := $(if $(QA_CHUNKS),--chunks,)

# The corpus is dated relative to the day it is seeded, so a profile seeded on
# an earlier day misses every "yesterday" query. QA_SKIP_SEED=1 is refused then.
qa-retrieval-check: $(if $(QA_SKIP_SEED),,qa-seed)
	@if [ "$$(cat "$(QA_PROFILE)/.seeded-on" 2>/dev/null)" != "$$(date +%F)" ]; then \
		echo "The QA profile was not seeded today, so its time queries would miss. Run without QA_SKIP_SEED=1." >&2; exit 1; \
	fi
	mkdir -p "$(QA_CHECK_DIR)"
	cd src-tauri && CARGO_BUILD_JOBS="$(CARGO_BUILD_JOBS)" cargo run --example retrieval_qa -- --data-dir "$(QA_PROFILE)" --cases "$(QA_QUERIES)" --out "$(QA_CHECK_DIR)/current.md" --json "$(QA_CHECK_DIR)/current.json" $(QA_CHUNK_ARGS) > /dev/null
	$(PYTHON) scripts/audit/retrieval_check.py --reference "$(QA_REFERENCE)" --current "$(QA_CHECK_DIR)/current.json" --out "$(QA_CHECK_DIR)/check.md"

# VS-02: PERSONA=<name> runs the QA targets on scripts/demo/<name>-week.json and
# <name>-queries.json in its own seeded profile, and writes its baseline under W03.
# Without PERSONA (or with PERSONA=knowledge-worker) nothing above changes.
ifneq ($(filter-out knowledge-worker,$(PERSONA)),)
QA_PROFILE := $(HOME)/Library/Application Support/com.fndr.app.qa-$(PERSONA)
QA_CORPUS := $(CURDIR)/scripts/demo/$(PERSONA)-week.json
QA_QUERIES := $(CURDIR)/scripts/demo/$(PERSONA)-queries.json
QA_RETRIEVAL_MD := $(CURDIR)/docs/evidence/W03/retrieval-baseline-$(PERSONA).md
QA_RETRIEVAL_JSON := $(CURDIR)/docs/evidence/W03/retrieval-baseline-$(PERSONA).json
endif

# PD-05: Friday scoreboard. Prints one Markdown page to stdout from the retrieval
# reference reports and the vault health evidence. Override any SCOREBOARD_*
# variable; files that do not exist print "not measured" instead of failing.
.PHONY: scoreboard
SCOREBOARD_RETRIEVAL ?= $(wildcard scripts/demo/retrieval-reference/*.json)
SCOREBOARD_VAULT_HEALTH ?= docs/evidence/W02/vault-health-owner.md

scoreboard:
	@$(PYTHON) scripts/audit/scoreboard.py $(foreach report,$(SCOREBOARD_RETRIEVAL),--retrieval "$(report)") $(if $(SCOREBOARD_VAULT_HEALTH),--vault-health "$(SCOREBOARD_VAULT_HEALTH)") $(if $(SCOREBOARD_VOICE),--voice "$(SCOREBOARD_VOICE)") $(if $(SCOREBOARD_SESSIONS),--sessions "$(SCOREBOARD_SESSIONS)") $(if $(SCOREBOARD_DATE),--date "$(SCOREBOARD_DATE)")
