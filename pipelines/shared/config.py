"""Shared configuration for indexing pipelines."""

from pathlib import Path
from dataclasses import dataclass, field

# Base paths - generic, no project-specific names
PROJECT_ROOT = Path(".")
COCOINDEX_DIR = PROJECT_ROOT / ".cocoindex_code"
SQLITE_PATH = COCOINDEX_DIR / "cocoindex.db"  # Internal metadata (LMDB)
TARGET_SQLITE_PATH = COCOINDEX_DIR / "target_sqlite.db"  # User data (SQLite)

# Include/exclude patterns
INCLUDE_PATTERNS = [
    "**/*.rs", "**/*.md", "**/*.py", "**/*.toml", "**/*.yaml", "**/*.yml",
    "**/*.json", "**/*.sh", "**/*.go", "**/*.java", "**/*.c", "**/*.h",
    "**/*.cpp", "**/*.ts", "**/*.tsx", "**/*.js", "**/*.jsx", "**/*.html",
    "**/*.css", "**/*.mdx", "**/*.sql",
]

EXCLUDE_PATTERNS = [
    "**/.*", "**/__pycache__", "**/node_modules", "**/target",
    "**/build", "**/dist", "**/vendor", "**/.cocoindex_code",
]

# Context key for database
SQLITE_DB = "app_db"


@dataclass
class PipelineConfig:
    """Configuration for indexing pipelines."""
    name: str = "code-indexer"
    db_path: Path = SQLITE_PATH
    include_patterns: list[str] = field(default_factory=lambda: INCLUDE_PATTERNS)
    exclude_patterns: list[str] = field(default_factory=lambda: EXCLUDE_PATTERNS)
    chunk_size: int = 1000
    chunk_overlap: int = 300
    min_chunk_size: int = 300
    live_mode: bool = True


DEFAULT_CONFIG = PipelineConfig()