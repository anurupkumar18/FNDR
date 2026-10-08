#!/usr/bin/env bash
set -euo pipefail

TARGET_DIR_DEFAULT="$HOME/Library/Application Support/com.fndr.app/models"
TARGET_DIR="${1:-$TARGET_DIR_DEFAULT}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

python3 - "$TARGET_DIR" "$SCRIPT_DIR/../../src-tauri/src/inference/model_config.rs" << 'PYEOF'
import hashlib
import os
from pathlib import Path
import re
import shutil
import sys
import tempfile
from urllib.request import urlopen


def verify(path, expected):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    if digest.hexdigest() != expected:
        raise ValueError(f"SHA-256 mismatch for {path.name}")


def install():
    target = Path(sys.argv[1]).expanduser()
    source = Path(sys.argv[2]).read_text()

    def constant(name):
        match = re.search(r'pub const ' + name + r': &str =\s*"([^"\n]+)";', source)
        if not match:
            raise ValueError(f"Missing model_config.rs constant: {name}")
        return match.group(1)

    assets = [tuple(constant(prefix + suffix) for suffix in
                    ("_FILENAME", "_DOWNLOAD_URL", "_SHA256"))
              for prefix in ("EMBEDDING_MODEL", "EMBEDDING_TOKENIZER")]
    target.mkdir(parents=True, exist_ok=True)
    missing = []
    for name, url, digest in assets:
        destination = target / name
        if destination.exists():
            try:
                verify(destination, digest)
            except ValueError as error:
                raise ValueError(
                    f"{error}. Existing assets were not changed. Use a separate model "
                    "directory to evaluate the pinned assets; changing an installed "
                    "tokenizer may require reindexing existing memories."
                ) from error
            print(f"Verified existing {name}")
        else:
            missing.append((name, url, digest))

    # Verify every missing asset before promoting any of them. A failed or
    # interrupted transfer cannot leave a partial file in the live model pair.
    with tempfile.TemporaryDirectory(prefix=".minilm-download-", dir=target) as staging:
        for name, url, digest in missing:
            staged = Path(staging) / name
            print(f"Downloading pinned {name}")
            with urlopen(url, timeout=60) as response, staged.open("wb") as output:
                shutil.copyfileobj(response, output)
            verify(staged, digest)
        for name, _, _ in missing:
            destination = target / name
            # Do not overwrite an asset another process installed meanwhile.
            os.link(Path(staging) / name, destination)
    print(f"Verified MiniLM assets ready at {target}")


try:
    install()
except (OSError, ValueError) as error:
    print(f"MiniLM installation failed: {error}", file=sys.stderr)
    sys.exit(1)
PYEOF
