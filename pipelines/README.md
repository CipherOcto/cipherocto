# Generic CocoIndex Pipelines

Universal pipelines that work with any project - no project-specific names.

## Structure

```
pipelines/
├── __init__.py
├── shared/
│   ├── __init__.py
│   ├── config.py      # Generic config (no project names)
│   └── models.py      # Data models
├── sources/
│   └── metadata_extraction.py
└── targets/
    ├── file_summary.py
    └── embedding_generator.py
```

## Usage

Copy to any project and run:

```bash
source .venv/bin/activate

# Main code indexer
cocoindex update cocoindex_main.py

# File summaries
cocoindex update pipelines/targets/file_summary.py

# Metadata extraction
cocoindex update pipelines/sources/metadata_extraction.py

# Embeddings
cocoindex update pipelines/targets/embedding_generator.py
```

## Key Features

- **No project names** - uses "app_db", "code-indexer", etc.
- **Generic patterns** - works with Rust, Python, TypeScript, Go, etc.
- **Self-contained** - copy verbatim between projects