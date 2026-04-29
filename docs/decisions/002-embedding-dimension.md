# 002: Embedding Dimension

FNDR's stable text embedding contract is 1024 dimensions. The current embedding path uses a local ONNX BGE-style model downloaded by `download_embedding_model.sh`, and the LanceDB text vector columns are created and validated against that dimension.

The dimension is intentionally treated as an application contract, not a casual runtime preference. Capture, meeting ingestion, downloads ingestion, search queries, snippet embeddings, support embeddings, and LanceDB schema validation all need to agree. If one subsystem silently writes 384-dimensional vectors while another expects 1024-dimensional vectors, vector search either fails at query time or returns misleading results.

Older prototype code used a MiniLM sidecar that produced 384-dimensional embeddings. That path is no longer part of the stable pipeline because it conflicts with the current schema and duplicates the native ONNX implementation. Keeping both paths would make failures look like search-quality problems when the real issue is schema/model mismatch.

FNDR validates this in two places. Configuration rejects non-1024 text embedding dimensions for this build, and LanceDB schema validation reports a clear error if an existing table has the wrong vector size. Incoming records are also normalized before indexing so malformed vectors are padded, truncated, or zero-filled rather than corrupting the table.
