---
name: cocoindex
description: CocoIndex for incremental data pipelines. Use when building code indexing, vector embeddings, knowledge graphs, or ETL workflows with automatic change detection. Pipelines are project-agnostic — COPY them to new projects, do NOT rewrite.
keywords: copy, replicate, clone, setup pipeline, new project, index project, add cocoindex, data pipeline, embedding, vector search, incremental
---

# CocoIndex

Python library for incremental data transformation. Think spreadsheets or React for data pipelines: declare what the output should look like based on current input, and CocoIndex handles incremental updates, change detection, and syncing.

## CRITICAL: Copy, Don't Rebuild

The pipelines under `pipelines/` and `cocoindex_main.py` are **project-agnostic**. When setting up CocoIndex in a new project:

1. **COPY** the files directly: `cp cocoindex_main.py <target>/ && cp -r pipelines/ <target>/`
2. **DO NOT** rewrite, adapt, or "port" them — they work as-is for any codebase
3. **DO NOT** read the files and then produce new files with the same logic
4. The only per-project setup is: `.env` file, `.cocoindex_code/` directory, `.gitignore` entries

**Red flags that you're about to do it wrong:**
- "Let me understand the pipeline structure first, then create one for this project"
- "This project is different because it uses Rust/Python/Go" — the pipelines already handle all languages
- "I need to adapt the config" — the config uses relative paths and generic patterns
- Reading all files to "learn the patterns" then writing new files = rewriting. Just copy.

If you find yourself reading pipeline source files and then writing new pipeline files, **STOP**. Use `cp` instead.

## Setup for a New Project

```bash
# 1. Copy pipelines from an existing project (e.g. cipherocto)
cp /path/to/source/cocoindex_main.py .
cp -r /path/to/source/pipelines/ .

# 2. Create .env
echo "COCOINDEX_DB=.cocoindex_code/cocoindex.db" > .env

# 3. Create data directory
mkdir -p .cocoindex_code

# 4. Add to .gitignore
echo ".cocoindex_code/" >> .gitignore
echo ".env" >> .gitignore
```

## CLI Commands

```bash
# List all apps
.venv/bin/cocoindex ls cocoindex_main.py

# Show app stable paths
.venv/bin/cocoindex show cocoindex_main.py

# Run incremental update (processes only changed files)
.venv/bin/cocoindex update -f cocoindex_main.py

# Live mode — keeps watching for changes
.venv/bin/cocoindex update -fL cocoindex_main.py

# Drop app and all target state
.venv/bin/cocoindex drop cocoindex_main.py

# Force reprocess everything
.venv/bin/cocoindex update -f --full-reprocess cocoindex_main.py

# Reset and reprocess
.venv/bin/cocoindex update -f --reset cocoindex_main.py
```

## Environment

Loads `.env` from current directory. Key variable:
- `COCOINDEX_DB` — path to internal state database (default: `./cocoindex.db`)

```bash
.venv/bin/cocoindex -e .env update -f cocoindex_main.py
```

## Core Concepts

### Apps

An **App** is the top-level executable that binds a main function with parameters:

```python
import cocoindex as coco

@coco.fn
async def app_main(sourcedir: pathlib.Path) -> None:
    ...

app = coco.App(
    coco.AppConfig(name="MyApp"),
    app_main,
    sourcedir=pathlib.Path("./data"),
)
```

### Functions (`@coco.fn`)

The `@coco.fn` decorator marks functions as CocoIndex processing functions. Add `memo=True` to skip re-execution when inputs/code unchanged:

```python
@coco.fn(memo=True)
async def expensive_operation(data: str) -> Result:
    return await expensive_transform(data)
```

### Lifespan (`@coco.lifespan`)

Use `@coco.lifespan` for setup/teardown of resources like database connections:

```python
@coco.lifespan
def my_lifespan(builder: coco.EnvironmentBuilder) -> Iterator[None]:
    with sqlite.managed_connection("target.db") as conn:
        builder.provide(SQLITE_DB, conn)
        yield
```

### Target States

Declare what should exist — CocoIndex handles creation/update/deletion:

```python
@dataclass
class MyRecord:
    id: int
    name: str

table.declare_row(row=MyRecord(id=1, name="example"))
```

### Connectors

| Connector | Source | Target | Vectors |
|-----------|--------|--------|---------|
| PostgreSQL | Y | Y | pgvector |
| SQLite | - | Y | sqlite-vec |
| LanceDB | - | Y | Y |
| Qdrant | - | Y | Y |
| LocalFS | Y | Y | N/A |
| S3 | Y | - | N/A |
| Kafka | Y | Y | N/A |

## App Structure

```python
import pathlib
from typing import Iterator
import cocoindex as coco
from cocoindex.connectors import localfs, sqlite
from cocoindex.ops.text import RecursiveSplitter, detect_code_language
from cocoindex.resources.chunk import Chunk
from cocoindex.resources.file import FileLike, PatternFilePathMatcher
from cocoindex.resources.id import IdGenerator

@coco.lifespan
def coco_lifespan(builder: coco.EnvironmentBuilder) -> Iterator[None]:
    with sqlite.managed_connection("target.db") as conn:
        builder.provide(SQLITE_DB, conn)
        yield

app = coco.App(coco.AppConfig(name="MyApp"), app_main)
```

## File Walking

```python
files = localfs.walk_dir(
    pathlib.Path("."),
    recursive=True,
    path_matcher=PatternFilePathMatcher(
        included_patterns=["**/*.py", "**/*.rs"],
        excluded_patterns=["**/target", "**/node_modules"],
    ),
)
```

## Text Splitting

```python
splitter = RecursiveSplitter()
language = detect_code_language(filename="example.py")
chunks = splitter.split(text, chunk_size=1000, chunk_overlap=200, language=language)
```

## Processing Pattern

```python
@coco.fn(memo=True)
async def process_file(file: FileLike, table: sqlite.TableTarget) -> None:
    text = await file.read_text()
    chunks = _splitter.split(text, chunk_size=1000, chunk_overlap=200)
    id_gen = IdGenerator()
    await coco.map(process_chunk, chunks, file.file_path.path, id_gen, table)

@coco.fn
async def process_chunk(
    chunk: Chunk, filename: pathlib.PurePath,
    id_gen: IdGenerator, table: sqlite.TableTarget,
) -> None:
    table.declare_row(row=MyDataclass(
        id=await id_gen.next_id(chunk.text),
        filename=str(filename),
        text=chunk.text,
    ))
```

## Wiring

```python
@coco.fn
async def app_main() -> None:
    files = localfs.walk_dir(...)
    table = await sqlite.mount_table_target(
        SQLITE_DB, "table_name",
        await sqlite.TableSchema.from_class(MyDataclass, primary_key=["id"]),
    )
    await coco.mount_each(process_file, files.items(), table)
```

## Best Practices

1. Use `@coco.fn` on all processing functions
2. Add `memo=True` for expensive operations (embeddings, LLM calls)
3. Use `@coco.lifespan` for database connections (not manual sqlite3.connect)
4. Use dataclasses for record types (not dict)
5. Enable WAL mode: `conn.execute("PRAGMA journal_mode=WAL")`
6. Store numpy arrays as JSON strings: `json.dumps(embedding.tolist())`

## Index Location (any project)

- `.cocoindex_code/cocoindex.db/` — internal state (LMDB)
- `.cocoindex_code/target_sqlite.db` — target output (SQLite)
- `.cocoindex_code/settings.yml` — file patterns
- `cocoindex_main.py` — app module

## Pipeline Structure

```
pipelines/
├── shared/
│   ├── config.py            # Generic configuration (no project names)
│   ├── models.py            # Data models
│   └── utils.py             # Shared search/embedding utilities (numpy vectorized)
├── sources/
│   ├── metadata_extraction.py   # Extract symbols (fn, class, struct, etc.)
│   ├── import_graph.py          # Map inter-file import dependencies
│   ├── api_extractor.py         # Extract HTTP API endpoints
│   └── test_indexer.py          # Index test files separately
└── targets/
    ├── file_summary.py          # Create file overview summaries
    ├── embedding_generator.py   # Generate vector embeddings for symbols
    ├── direct_embed.py          # Generate chunk embeddings (sync, fast)
    ├── similarity_search.py     # Re-exports from shared/utils.py
    └── search_cli.py            # Semantic search CLI (chunks + symbols)

cocoindex_main.py               # Main code indexer (chunking)
```

## Pipeline Registry

### Source Pipelines (write to target_sqlite.db)

| Pipeline | App Name | File | Target Table | What It Does |
|----------|----------|------|--------------|--------------|
| Main indexer | `code-indexer` | `cocoindex_main.py` | `code_chunks` | Walks files, splits into chunks with line ranges |
| Metadata extraction | `metadata-extraction` | `pipelines/sources/metadata_extraction.py` | `code_symbols` | Extracts function/class/struct names via regex |
| Import graph | `import-graph` | `pipelines/sources/import_graph.py` | `import_graph` | Maps `use`/`import`/`from` dependencies between files |
| API extractor | `api-extractor` | `pipelines/sources/api_extractor.py` | `api_endpoints` | Finds HTTP endpoints (FastAPI, Express, Actix, Go) |
| Test indexer | `test-indexer` | `pipelines/sources/test_indexer.py` | `test_index` | Indexes test functions by framework (pytest, jest, rust) |

### Target Pipelines (read from + write to target_sqlite.db)

| Pipeline | App Name | File | Reads From | Writes To |
|----------|----------|------|------------|-----------|
| File summaries | `file-summaries` | `pipelines/targets/file_summary.py` | source files | `file_summaries` |
| Symbol embeddings | `symbol-embeddings` | `pipelines/targets/embedding_generator.py` | `code_symbols` | `symbol_embeddings` |
| Direct embed | — | `pipelines/targets/direct_embed.py` | `code_chunks` | `code_chunks_embeddings` |

### Search Tools (read-only)

| Tool | File | Reads From |
|------|------|------------|
| Search CLI | `pipelines/targets/search_cli.py` | `code_chunks_embeddings`, `symbol_embeddings` |
| Similarity search | `pipelines/targets/similarity_search.py` | same (re-exports from utils) |

## Database Schema

**8 tables in `.cocoindex_code/target_sqlite.db`:**

| Table | Columns | Primary Key |
|-------|---------|-------------|
| `code_chunks` | id, filename, code, start_line, end_line | id |
| `code_symbols` | name, kind, file_path, line, signature, language | (name, kind, file_path) |
| `file_summaries` | path, file_type, category, size_bytes, line_count, symbol_count, import_count, has_docs, first_heading | path |
| `import_graph` | source_file, target_module, import_type, line_number | (source_file, target_module, line_number) |
| `api_endpoints` | file_path, method, path, handler, line_number | (file_path, method, path, line_number) |
| `test_index` | file_path, test_name, test_type, framework, line_number, content | (file_path, test_name, line_number) |
| `symbol_embeddings` | name, kind, file_path, line, signature, language, embedding (JSON) | (name, kind, file_path) |
| `code_chunks_embeddings` | chunk_id, filename, chunk_text, start_line, end_line, embedding (JSON), created_at | chunk_id |

## Dependencies & Execution Order

```
Phase 1: Source indexing (all independent, run in any order)
┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐
│  code-indexer     │  │ metadata-extract  │  │  import-graph    │  │  api-extractor   │  │  test-indexer    │
│  cocoindex_main   │  │ sources/metadata  │  │  sources/import  │  │  sources/api     │  │  sources/test    │
│  → code_chunks    │  │  → code_symbols   │  │  → import_graph  │  │  → api_endpoints │  │  → test_index    │
└──────────────────┘  └────────┬─────────┘  └──────────────────┘  └──────────────────┘  └──────────────────┘
                               │
Phase 2: Derived data           │ reads code_symbols
                               ▼
                      ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐
                      │ symbol-embeddings │  │   direct_embed   │  │  file-summaries  │
                      │ targets/embedding │  │  targets/direct  │  │  targets/file    │
                      │ → symbol_embedd.. │  │ → code_chunks_.. │  │  → file_summaries│
                      └──────────────────┘  └──────────────────┘  └──────────────────┘
                               │                    │
Phase 3: Search                 │ reads both         │
                               ▼                    ▼
                      ┌──────────────────────────────────────┐
                      │  search_cli / similarity_search       │
                      │  reads: code_chunks_embeddings,       │
                      │         symbol_embeddings              │
                      └──────────────────────────────────────┘
```

### Required execution order

```bash
# Phase 1 — source indexing (independent, can run in parallel)
.venv/bin/cocoindex update -f cocoindex_main.py
.venv/bin/cocoindex update -f pipelines/sources/metadata_extraction.py
.venv/bin/cocoindex update -f pipelines/sources/import_graph.py
.venv/bin/cocoindex update -f pipelines/sources/api_extractor.py
.venv/bin/cocoindex update -f pipelines/sources/test_indexer.py

# Phase 2 — embeddings & summaries (depends on Phase 1)
.venv/bin/cocoindex update -f pipelines/targets/embedding_generator.py   # needs code_symbols
python pipelines/targets/direct_embed.py                                 # needs code_chunks
.venv/bin/cocoindex update -f pipelines/targets/file_summary.py          # reads source files directly

# Phase 3 — search (depends on Phase 2)
python pipelines/targets/search_cli.py "your query"
```

### Key dependency rules

1. **`symbol_embeddings` depends on `code_symbols`** — embedding_generator joins against code_symbols to find unembedded symbols
2. **`code_chunks_embeddings` depends on `code_chunks`** — direct_embed joins against code_chunks to find unembedded chunks
3. **`file_summaries` has no table dependencies** — reads source files directly, can run in Phase 1
4. **Search tools depend on both `*_embeddings` tables** — need Phase 2 complete

## Semantic Search

Two search modes available via `search_cli.py` or `pipelines.shared.utils`:

```bash
# Unified search (chunks + symbols, ranked by score)
python pipelines/targets/search_cli.py "your query"

# Chunks only
python pipelines/targets/search_cli.py "your query" --chunks

# Symbols only
python pipelines/targets/search_cli.py "your query" --symbols

# JSON output for programmatic use
python pipelines/targets/search_cli.py "your query" --json -k 20 -t 0.3
```

Python API:
```python
from pipelines.shared.utils import search_chunks, search_symbols, search_unified

results = search_unified("your query", top_k=10, threshold=0.25)
# Returns: [{"type": "chunk"|"symbol", "score": 0.65, ...}, ...]
```

## Shared Config

```python
# pipelines/shared/config.py
TARGET_SQLITE_PATH = ".cocoindex_code/target_sqlite.db"
EMBEDDING_MODEL = "sentence-transformers/all-MiniLM-L6-v2"  # 384 dims
CHUNK_SIZE = 1000
CHUNK_OVERLAP = 300
MIN_CHUNK_SIZE = 300
```

## Version

CocoIndex `>=1.0.0` (v1).

## Resources

- [Docs](https://cocoindex.io/docs)
- [GitHub](https://github.com/cocoindex-io/cocoindex)
- [Examples](https://github.com/cocoindex-io/cocoindex/tree/main/examples)
