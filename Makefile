.PHONY: demo install dev test rust-test

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
