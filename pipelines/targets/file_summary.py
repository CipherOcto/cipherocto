"""File summary index - creates overview metadata for quick navigation."""

import pathlib
from dataclasses import dataclass
from typing import Iterator

import cocoindex as coco
from cocoindex.connectors import localfs, sqlite
from cocoindex.ops.text import detect_code_language
from cocoindex.resources.file import FileLike, PatternFilePathMatcher

from pipelines.shared.config import TARGET_SQLITE_PATH, INCLUDE_PATTERNS, EXCLUDE_PATTERNS

SQLITE_DB = coco.ContextKey[sqlite.ManagedConnection]("app_db")


@dataclass
class FileSummary:
    path: str
    file_type: str
    category: str
    size_bytes: int
    line_count: int
    symbol_count: int
    import_count: int
    has_docs: bool
    first_heading: str


@coco.lifespan
def summary_lifespan(builder: coco.EnvironmentBuilder) -> Iterator[None]:
    with sqlite.managed_connection(str(TARGET_SQLITE_PATH)) as conn:
        builder.provide(SQLITE_DB, conn)
        yield


# Module-level table reference
_table: sqlite.TableTarget | None = None


async def get_table() -> sqlite.TableTarget:
    global _table
    if _table is None:
        _table = await sqlite.mount_table_target(
            SQLITE_DB,
            "file_summaries",
            await sqlite.TableSchema.from_class(FileSummary, primary_key=["path"]),
        )
    return _table


@coco.fn(memo=True)
async def process_file(file: FileLike) -> None:
    """Process a file to create summary metadata."""
    content = await file.read_text()
    path = str(file.file_path.path)
    language = detect_code_language(filename=path)

    lines = content.split("\n")

    # Count symbols
    symbol_count = sum(1 for line in lines
        if any(kw in line for kw in ["fn ", "class ", "struct ", "def ", "interface ", "impl ", "trait ", "pub ", "mod "]))

    # Count imports
    import_count = sum(1 for line in lines
        if any(imp in line for imp in ["import ", "use ", "from ", "require(", "include("]))

    # Check for docs
    has_docs = ("<summary>" in content or "<doc>" in content or
                '"""' in content or "///" in content or "/**" in content or "//!")

    # Extract first heading
    first_heading = ""
    for line in lines[:20]:
        if line.startswith("# "):
            first_heading = line[2:].strip()[:100]
            break

    # Determine category from universal conventions
    p = path.lower()
    if "/test" in p or "/tests" in p or "test_" in p or "_test." in p or ".test." in p or ".spec." in p:
        category = "test"
    elif any(p.endswith(ext) for ext in (".md", ".mdx", ".rst", ".txt")):
        category = "doc"
    elif any(p.endswith(ext) for ext in (".toml", ".yaml", ".yml", ".json", ".ini", ".cfg")):
        category = "config"
    elif language and language != "unknown":
        category = "code"
    else:
        category = "other"

    # Store summary
    table = await get_table()
    table.declare_row(row=FileSummary(
        path=path,
        file_type=language,
        category=category,
        size_bytes=len(content.encode()),
        line_count=len(lines),
        symbol_count=symbol_count,
        import_count=import_count,
        has_docs=has_docs,
        first_heading=first_heading,
    ))


@coco.fn
async def app_main() -> None:
    """Main entry point for file summary generation."""
    files = localfs.walk_dir(
        pathlib.Path("."),
        recursive=True,
        path_matcher=PatternFilePathMatcher(
            included_patterns=INCLUDE_PATTERNS,
            excluded_patterns=EXCLUDE_PATTERNS,
        ),
        live=True,
    )

    await coco.mount_each(process_file, files.items())


app = coco.App(
    coco.AppConfig(name="file-summaries"),
    app_main,
)