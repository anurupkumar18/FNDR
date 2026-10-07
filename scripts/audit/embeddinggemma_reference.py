#!/usr/bin/env python3
"""Reference vectors for VS-47: EmbeddingGemma through sentence-transformers.

Writes src-tauri/tests/fixtures/embeddinggemma_reference.json: twenty synthetic
sentences (ten queries, ten documents), each embedded by the reference
implementation with the model card's retrieval prompts. The Rust ONNX embedder
must match them (tests/embeddinggemma_reference.rs).

Run in the bake-off environment (scripts/audit/requirements-bakeoff.txt):

  python scripts/audit/embeddinggemma_reference.py

The weights come from the ungated mirror at a pinned revision; every weight file
has the same sha256 as google/embeddinggemma-300m (see embedding_bakeoff.py).
"""

from __future__ import annotations

import json
from pathlib import Path

from sentence_transformers import SentenceTransformer
import sentence_transformers

MODEL = "unsloth/embeddinggemma-300m"
REVISION = "bfa3c846ac738e62aa61806ef9112d34acb1dc5a"
QUERY_PROMPT = "task: search result | query: "
DOCUMENT_PROMPT = "title: none | text: "
OUT = Path(__file__).resolve().parents[2] / "src-tauri/tests/fixtures/embeddinggemma_reference.json"

QUERIES = [
    "why did the tracking API get slow after the release",
    "how do we stop a retried request from buying two shipping labels",
    "which plotting library am I allowed to use",
    "the essay draft about labor contracts",
    "how much money are we losing to customers leaving",
    "Postgres 16 runbook due October 9",
    "what did my manager say I need to get promoted",
    "pandas import error fix",
    "réunion budget trimestriel avec l'équipe",
    "LL-1482 spam placement 1.8%",
]
DOCUMENTS = [
    "INC-2291 postmortem: 38 minutes of p99 above 2 s on GET /v2/shipments, caused by an N+1 carrier query.",
    "Add idempotency keys to POST /v2/labels so a retried request returns the stored label.",
    "Course policy: use matplotlib for every chart in the assignment; seaborn is not allowed.",
    "Reconstruction essay draft, second paragraph on labor contracts and the Freedmen's Bureau.",
    "Q3 churn cost us $1.24M in ARR; Owen split it by segment in the metrics sheet.",
    "Postgres 16 upgrade runbook. Owner: Sam K. Due: October 9. Method: logical replication.",
    "1:1 with Grace: the promo packet needs a design you led and an incident you ran.",
    "ModuleNotFoundError: No module named 'pandas' after creating the new virtualenv.",
    "Compte rendu de la réunion budgétaire du trimestre, avec les décisions de l'équipe.",
    "Spam placement for LL-1482 fell from 6.4% to 1.8% after the DKIM fix on staging.",
]


def main() -> None:
    model = SentenceTransformer(MODEL, revision=REVISION, device="cpu")
    query_vectors = model.encode(QUERIES, prompt=QUERY_PROMPT, normalize_embeddings=True)
    document_vectors = model.encode(DOCUMENTS, prompt=DOCUMENT_PROMPT, normalize_embeddings=True)
    items = [
        {"kind": "query", "text": text, "embedding": [round(float(x), 7) for x in vector]}
        for text, vector in zip(QUERIES, query_vectors)
    ] + [
        {"kind": "document", "text": text, "embedding": [round(float(x), 7) for x in vector]}
        for text, vector in zip(DOCUMENTS, document_vectors)
    ]
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(
        json.dumps(
            {
                "model": MODEL,
                "revision": REVISION,
                "library": f"sentence-transformers {sentence_transformers.__version__}",
                "query_prompt": QUERY_PROMPT,
                "document_prompt": DOCUMENT_PROMPT,
                "dimensions": len(items[0]["embedding"]),
                "items": items,
            },
            ensure_ascii=False,
        )
        + "\n"
    )
    print(f"wrote {len(items)} reference vectors of {len(items[0]['embedding'])} dimensions to {OUT}")


if __name__ == "__main__":
    main()
