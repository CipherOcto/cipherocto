"""Vector embedding pipeline - generates embeddings for code symbols."""

from dataclasses import dataclass
from typing import Iterator, Optional
import json

import cocoindex as coco
from cocoindex.connectors import sqlite

from pipelines.shared.config import TARGET_SQLITE_PATH

SQLITE_DB = coco.ContextKey[sqlite.ManagedConnection]("app_db")

# Configuration
EMBEDDING_MODEL = "sentence-transformers/all-MiniLM-L6-v2"
BATCH_SIZE = 32


@dataclass
class SymbolEmbedding:
    name: str
    kind: str
    file_path: str
    line: int
    signature: str
    language: str
    embedding: str  # JSON string of embedding vector


def get_embedder():
    """Get embedding model."""
    from cocoindex.ops.sentence_transformers import SentenceTransformerEmbedder
    return SentenceTransformerEmbedder(EMBEDDING_MODEL)


@coco.lifespan
def embed_lifespan(builder: coco.EnvironmentBuilder) -> Iterator[None]:
    with sqlite.managed_connection(str(TARGET_SQLITE_PATH)) as conn:
        builder.provide(SQLITE_DB, conn)
        yield


@coco.fn(memo=True)
async def embed_symbol(row: dict, embedder) -> Optional[SymbolEmbedding]:
    """Generate embedding for a single symbol."""
    try:
        text = f"{row['kind']} {row['name']}: {row['signature']}"
        embedding = await embedder.embed(text)
        emb_list = embedding.tolist() if hasattr(embedding, 'tolist') else list(embedding)
        return SymbolEmbedding(
            name=row["name"],
            kind=row["kind"],
            file_path=row["file_path"],
            line=row["line"],
            signature=row["signature"],
            language=row["language"],
            embedding=json.dumps(emb_list),
        )
    except Exception as e:
        print(f"Embedding error for {row['name']}: {e}")
        return None


@coco.fn
async def store_embeddings(chunks: list, table: sqlite.TableTarget) -> None:
    """Store embeddings in database."""
    for chunk in chunks:
        if chunk:
            table.declare_row(row=chunk)


@coco.fn
async def app_main() -> None:
    """Main entry point for embedding generation."""
    embedder = get_embedder()
    print(f"Using embedder: {EMBEDDING_MODEL}")

    # Query symbols without embeddings via raw connection
    import sqlite3
    conn = sqlite3.connect(str(TARGET_SQLITE_PATH))
    cursor = conn.cursor()

    # Check if symbol_embeddings table exists
    cursor.execute("""
        SELECT name FROM sqlite_master WHERE type='table' AND name='symbol_embeddings'
    """)
    table_exists = cursor.fetchone() is not None

    # Get symbols that need embeddings
    if table_exists:
        cursor.execute("""
            SELECT cs.name, cs.kind, cs.file_path, cs.line, cs.signature, cs.language
            FROM code_symbols cs
            LEFT JOIN symbol_embeddings se 
                ON cs.name = se.name AND cs.kind = se.kind AND cs.file_path = se.file_path
            WHERE se.name IS NULL
        """)
    else:
        cursor.execute("SELECT name, kind, file_path, line, signature, language FROM code_symbols")

    rows = cursor.fetchall()
    symbols_data = [
        {"name": r[0], "kind": r[1], "file_path": r[2], "line": r[3], "signature": r[4], "language": r[5]}
        for r in rows
    ]
    conn.close()

    if not symbols_data:
        print("No symbols to embed")
        return

    print(f"Embedding {len(symbols_data)} symbols...")

    # Mount embeddings table
    table = await sqlite.mount_table_target(
        SQLITE_DB,
        "symbol_embeddings",
        await sqlite.TableSchema.from_class(SymbolEmbedding, primary_key=["name", "kind", "file_path"]),
    )

    # Process in batches
    for i in range(0, len(symbols_data), BATCH_SIZE):
        batch = symbols_data[i:i + BATCH_SIZE]
        embeddings = []

        for row in batch:
            result = await embed_symbol(row, embedder)
            if result:
                embeddings.append(result)

        await store_embeddings(embeddings, table)
        print(f"  Progress: {min(i + BATCH_SIZE, len(symbols_data))}/{len(symbols_data)}")

    print("Embedding complete!")


app = coco.App(
    coco.AppConfig(name="symbol-embeddings"),
    app_main,
)