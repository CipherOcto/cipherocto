#!/usr/bin/env python3
"""Direct embedding generator - simple sync version for speed."""

import json
import sqlite3
from pathlib import Path

# Configuration
DB_PATH = ".cocoindex_code/target_sqlite.db"
EMBEDDING_MODEL = "sentence-transformers/all-MiniLM-L6-v2"
BATCH_SIZE = 64


def main():
    """Generate embeddings for all chunks."""
    from sentence_transformers import SentenceTransformer

    print(f"Loading model: {EMBEDDING_MODEL}")
    embedder = SentenceTransformer(EMBEDDING_MODEL)

    # Connect to database
    conn = sqlite3.connect(DB_PATH)
    cursor = conn.cursor()

    # Check/create table
    cursor.execute("""
        CREATE TABLE IF NOT EXISTS code_chunks_embeddings (
            chunk_id INTEGER PRIMARY KEY,
            filename TEXT,
            chunk_text TEXT,
            start_line INTEGER,
            end_line INTEGER,
            embedding TEXT,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )
    """)
    cursor.execute("CREATE INDEX IF NOT EXISTS idx_cce_filename ON code_chunks_embeddings(filename)")
    cursor.execute("CREATE INDEX IF NOT EXISTS idx_cc_filename ON code_chunks(filename)")
    conn.commit()

    # Get chunks without embeddings
    cursor.execute("""
        SELECT cc.id, cc.filename, cc.code, cc.start_line, cc.end_line
        FROM code_chunks cc
        LEFT JOIN code_chunks_embeddings ce ON cc.id = ce.chunk_id
        WHERE ce.chunk_id IS NULL
    """)
    chunks = cursor.fetchall()
    total = len(chunks)

    if total == 0:
        print("All chunks already have embeddings")
        conn.close()
        return

    print(f"Embedding {total} chunks...")

    for i in range(0, total, BATCH_SIZE):
        batch = chunks[i:i + BATCH_SIZE]
        chunk_ids = [r[0] for r in batch]
        filenames = [r[1] for r in batch]
        codes = [r[2] for r in batch]
        start_lines = [r[3] for r in batch]
        end_lines = [r[4] for r in batch]

        # Embed batch
        embeddings = embedder.encode(codes)

        # Convert to list
        emb_list = embeddings.tolist()

        # Store embeddings
        for j, emb in enumerate(emb_list):
            cursor.execute("""
                INSERT OR REPLACE INTO code_chunks_embeddings 
                (chunk_id, filename, chunk_text, start_line, end_line, embedding)
                VALUES (?, ?, ?, ?, ?, ?)
            """, (chunk_ids[j], filenames[j], codes[j], start_lines[j], end_lines[j], json.dumps(emb)))

        conn.commit()
        progress = min(i + BATCH_SIZE, total)
        pct = (progress / total) * 100
        print(f"  {progress}/{total} ({pct:.1f}%)")

    conn.close()
    print(f"Done! Embedded {total} chunks.")


if __name__ == "__main__":
    main()