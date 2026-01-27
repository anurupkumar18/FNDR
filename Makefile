.PHONY: dev build clean reset-db install

# Development
dev:
	npm run tauri dev

# Build production app
build:
	npm run tauri build

# Install dependencies
install:
	npm install
	cd src-tauri && cargo build

# Clean build artifacts
clean:
	rm -rf dist
	rm -rf src-tauri/target
	rm -rf node_modules

# Reset database
reset-db:
	rm -rf ~/Library/Application\ Support/com.fndr.app/lancedb
	@echo "Database reset complete"

# Download embedding model
download-model:
	@mkdir -p ~/Library/Application\ Support/com.fndr.FNDR/models
	@echo "Please download the MiniLM ONNX model and tokenizer to:"
	@echo "  ~/Library/Application Support/com.fndr.FNDR/models/minilm.onnx"
	@echo "  ~/Library/Application Support/com.fndr.FNDR/models/tokenizer.json"
	@echo ""
	@echo "You can get them from:"
	@echo "  https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2"

# Run Rust tests
test:
	cd src-tauri && cargo test

# Check Rust code
check:
	cd src-tauri && cargo check
	cd src-tauri && cargo clippy

# Format code
fmt:
	cd src-tauri && cargo fmt
	npm run prettier -- --write src/
