"""Shared utilities for embedding and search pipelines."""

import json
import sqlite3
from functools import lru_cache
from typing import Optional

import numpy as np

from pipelines.shared.config import TARGET_SQLITE_PATH

EMBEDDING_MODEL = "sentence-transformers/all-MiniLM-L6-v2"


def get_embedder():
    """Get embedding model (shared instance)."""
    from sentence_transformers import SentenceTransformer
    return SentenceTransformer(EMBEDDING_MODEL)


def get_cocoindex_embedder():
    """Get CocoIndex-compatible embedder (async)."""
    from cocoindex.ops.sentence_transformers import SentenceTransformerEmbedder
    return SentenceTransformerEmbedder(EMBEDDING_MODEL)


def cosine_similarity_numpy(query_vec: np.ndarray, matrix: np.ndarray) -> np.ndarray:
    """Vectorized cosine similarity: query (d,) vs matrix (N, d) -> scores (N,)."""
    query_norm = query_vec / (np.linalg.norm(query_vec) + 1e-10)
    norms = np.linalg.norm(matrix, axis=1, keepdims=True) + 1e-10
    matrix_norm = matrix / norms
    return matrix_norm @ query_norm


def load_chunk_embeddings(
    db_path: str = str(TARGET_SQLITE_PATH),
) -> tuple[list[int], list[dict], np.ndarray]:
    """Load all chunk embeddings into a numpy matrix.

    Returns:
        (ids, metadata_list, matrix) where metadata_list has keys:
        chunk_id, filename, chunk_text, start_line, end_line
    """
    conn = sqlite3.connect(db_path)
    rows = conn.execute(
        "SELECT chunk_id, filename, chunk_text, start_line, end_line, embedding "
        "FROM code_chunks_embeddings"
    ).fetchall()
    conn.close()

    ids = []
    metadata = []
    vectors = []
    for chunk_id, filename, chunk_text, start_line, end_line, emb_json in rows:
        vec = json.loads(emb_json)
        ids.append(chunk_id)
        metadata.append({
            "chunk_id": chunk_id,
            "filename": filename,
            "chunk_text": chunk_text,
            "start_line": start_line,
            "end_line": end_line,
        })
        vectors.append(vec)

    matrix = np.array(vectors, dtype=np.float32) if vectors else np.empty((0, 384), dtype=np.float32)
    return ids, metadata, matrix


def load_symbol_embeddings(
    db_path: str = str(TARGET_SQLITE_PATH),
) -> tuple[list[tuple[str, str, str]], list[dict], np.ndarray]:
    """Load all symbol embeddings into a numpy matrix.

    Returns:
        (keys, metadata_list, matrix) where keys are (name, kind, file_path)
        and metadata_list has keys: name, kind, file_path, signature, line
    """
    conn = sqlite3.connect(db_path)
    rows = conn.execute(
        "SELECT name, kind, file_path, line, signature, embedding "
        "FROM symbol_embeddings"
    ).fetchall()
    conn.close()

    keys = []
    metadata = []
    vectors = []
    for name, kind, file_path, line, signature, emb_json in rows:
        vec = json.loads(emb_json)
        keys.append((name, kind, file_path))
        metadata.append({
            "name": name,
            "kind": kind,
            "file_path": file_path,
            "line": line,
            "signature": signature,
        })
        vectors.append(vec)

    matrix = np.array(vectors, dtype=np.float32) if vectors else np.empty((0, 384), dtype=np.float32)
    return keys, metadata, matrix


def search_chunks(
    query: str,
    db_path: str = str(TARGET_SQLITE_PATH),
    top_k: int = 10,
    threshold: float = 0.25,
) -> list[dict]:
    """Semantic search over code chunks."""
    embedder = get_embedder()
    query_vec = embedder.encode([query])[0].astype(np.float32)

    ids, metadata, matrix = load_chunk_embeddings(db_path)
    if matrix.shape[0] == 0:
        return []

    scores = cosine_similarity_numpy(query_vec, matrix)
    mask = scores >= threshold
    indices = np.where(mask)[0]
    indices = indices[np.argsort(scores[indices])[::-1][:top_k]]

    results = []
    for i in indices:
        m = metadata[i]
        results.append({
            "type": "chunk",
            "score": round(float(scores[i]), 3),
            "filename": m["filename"],
            "start_line": m["start_line"],
            "end_line": m["end_line"],
            "text": m["chunk_text"][:300],
        })
    return results


def search_symbols(
    query: str,
    db_path: str = str(TARGET_SQLITE_PATH),
    top_k: int = 10,
    threshold: float = 0.25,
) -> list[dict]:
    """Semantic search over code symbols."""
    embedder = get_embedder()
    query_vec = embedder.encode([query])[0].astype(np.float32)

    keys, metadata, matrix = load_symbol_embeddings(db_path)
    if matrix.shape[0] == 0:
        return []

    scores = cosine_similarity_numpy(query_vec, matrix)
    mask = scores >= threshold
    indices = np.where(mask)[0]
    indices = indices[np.argsort(scores[indices])[::-1][:top_k]]

    results = []
    for i in indices:
        m = metadata[i]
        results.append({
            "type": "symbol",
            "score": round(float(scores[i]), 3),
            "name": m["name"],
            "kind": m["kind"],
            "file_path": m["file_path"],
            "line": m["line"],
            "signature": m["signature"][:120],
        })
    return results


def search_unified(
    query: str,
    db_path: str = str(TARGET_SQLITE_PATH),
    top_k: int = 10,
    threshold: float = 0.25,
) -> list[dict]:
    """Semantic search over both chunks and symbols, unified by score."""
    embedder = get_embedder()
    query_vec = embedder.encode([query])[0].astype(np.float32)

    results = []

    # Search chunks
    cids, cmeta, cmatrix = load_chunk_embeddings(db_path)
    if cmatrix.shape[0] > 0:
        cscores = cosine_similarity_numpy(query_vec, cmatrix)
        for i in range(len(cmeta)):
            if cscores[i] >= threshold:
                m = cmeta[i]
                results.append({
                    "type": "chunk",
                    "score": round(float(cscores[i]), 3),
                    "filename": m["filename"],
                    "start_line": m["start_line"],
                    "end_line": m["end_line"],
                    "text": m["chunk_text"][:300],
                })

    # Search symbols
    skeys, smeta, smatrix = load_symbol_embeddings(db_path)
    if smatrix.shape[0] > 0:
        sscores = cosine_similarity_numpy(query_vec, smatrix)
        for i in range(len(smeta)):
            if sscores[i] >= threshold:
                m = smeta[i]
                results.append({
                    "type": "symbol",
                    "score": round(float(sscores[i]), 3),
                    "name": m["name"],
                    "kind": m["kind"],
                    "file_path": m["file_path"],
                    "line": m["line"],
                    "signature": m["signature"][:120],
                })

    results.sort(key=lambda x: x["score"], reverse=True)
    return results[:top_k]
