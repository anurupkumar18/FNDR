.PHONY: demo install dev test rust-test clean-dev-cache clean-dev-cache-dry-run

install:
	npm install

demo: install
	npm run tauri dev

test:
	npm run typecheck
	npm test
	cd src-tauri && cargo test

rust-test:
	cd src-tauri && cargo fmt --check && cargo clippy --all-targets && cargo test

clean-dev-cache:
	./scripts/clean-dev-build-cache.sh --yes

clean-dev-cache-dry-run:
	./scripts/clean-dev-build-cache.sh --dry-run
