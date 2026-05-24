"""Import graph indexer - maps dependencies between code modules."""

from dataclasses import dataclass
from typing import Iterator
import re
from pathlib import Path

import cocoindex as coco
from cocoindex.connectors import localfs, sqlite

from pipelines.shared.config import TARGET_SQLITE_PATH, PROJECT_ROOT

SQLITE_DB = coco.ContextKey[sqlite.ManagedConnection]("app_db")


@dataclass
class ImportEdge:
    """Represents an import dependency between files."""
    source_file: str
    target_module: str
    import_type: str  # "import" or "from"
    line_number: int


# Regex patterns for common languages
IMPORT_PATTERNS = {
    "python": [
        (r"^(?:from|import)\s+([a-zA-Z_][a-zA-Z0-9_.]*)", "python"),
    ],
    "rust": [
        (r"^use\s+([a-zA-Z_][a-zA-Z0-9_:]*)", "rust"),
    ],
    "javascript": [
        (r"^import\s+(?:{[^}]+}|[\w*]+)\s+from\s+['\"]([^'\"]+)['\"]", "js-from"),
        (r"^import\s+['\"]([^'\"]+)['\"]", "js-default"),
        (r"^require\(['\"]([^'\"]+)['\"]\)", "js-require"),
    ],
    "go": [
        (r"^import\s+(?:\"[^\"]+\"|\(\s*\n[^\)]+\))", "go-import"),
    ],
}


def detect_language(filename: str) -> str | None:
    """Detect programming language from file extension."""
    ext = Path(filename).suffix.lower()
    mapping = {
        ".py": "python",
        ".rs": "rust",
        ".js": "javascript",
        ".ts": "javascript",
        ".jsx": "javascript",
        ".tsx": "javascript",
        ".go": "go",
    }
    return mapping.get(ext)


def parse_imports(content: str, filename: str) -> list[ImportEdge]:
    """Parse import statements from file content."""
    edges = []
    lang = detect_language(filename)
    if not lang:
        return edges

    patterns = IMPORT_PATTERNS.get(lang, [])

    for line_num, line in enumerate(content.split("\n"), 1):
        line = line.strip()
        for pattern, imp_type in patterns:
            match = re.match(pattern, line)
            if match:
                target = match.group(1) if match.lastindex else match.group(0)
                edges.append(ImportEdge(
                    source_file=filename,
                    target_module=target,
                    import_type=imp_type,
                    line_number=line_num,
                ))
                break

    return edges


@coco.lifespan
def import_graph_lifespan(builder: coco.EnvironmentBuilder) -> Iterator[None]:
    with sqlite.managed_connection(str(TARGET_SQLITE_PATH)) as conn:
        builder.provide(SQLITE_DB, conn)
        yield


@coco.fn
async def app_main() -> None:
    """Index import dependencies across the codebase."""
    source_dir = PROJECT_ROOT

    # Get all code files
    code_files = []
    for ext in [".py", ".rs", ".js", ".ts", ".go"]:
        code_files.extend(source_dir.rglob(f"*{ext}"))

    # Filter out excluded directories
    excluded_dirs = {"node_modules", "target", "vendor", ".git", ".venv", "venv", "__pycache__"}
    code_files = [f for f in code_files if not any(part in f.parts for part in excluded_dirs)]

    print(f"Indexing {len(code_files)} files for imports...")

    # Mount the import_graph table
    schema = await sqlite.TableSchema.from_class(ImportEdge, primary_key=["source_file", "target_module", "line_number"])
    table = await sqlite.mount_table_target(SQLITE_DB, "import_graph", schema)

    indexed = 0
    for filepath in code_files:
        try:
            content = filepath.read_text(encoding="utf-8", errors="ignore")
            edges = parse_imports(content, str(filepath.relative_to(source_dir)))

            for edge in edges:
                table.declare_row(row=edge)

            if edges:
                indexed += 1
        except Exception as e:
            continue

    print(f"Indexed imports from {indexed} files")


app = coco.App(
    coco.AppConfig(name="import-graph"),
    app_main,
)