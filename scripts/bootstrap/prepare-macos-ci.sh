#!/usr/bin/env bash
set -euo pipefail

: "${GITHUB_ENV:?This bootstrap is for GitHub Actions}"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

# lance-encoding needs protoc. llama.cpp's bundled HTTP helper needs OpenSSL 3.
brew install protobuf openssl@3
openssl_prefix="$(brew --prefix openssl@3)"
{
  printf 'OPENSSL_ROOT_DIR=%s\n' "$openssl_prefix"
  printf 'PKG_CONFIG_PATH=%s/lib/pkgconfig%s\n' "$openssl_prefix" "${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
} >> "$GITHUB_ENV"
"$openssl_prefix/bin/openssl" version

# Cached CMake files retain the previous runner's Cellar and SDK paths.
# Rebuild this native package while keeping the rest of the Rust cache.
cargo clean --locked --manifest-path "$repo_root/src-tauri/Cargo.toml" --package llama-cpp-sys-2
