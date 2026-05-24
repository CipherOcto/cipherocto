"""Metadata extraction pipeline - extracts symbols, imports, docstrings."""

import pathlib
import re
from dataclasses import dataclass
from typing import Iterator

import cocoindex as coco
from cocoindex.connectors import localfs, sqlite
from cocoindex.ops.text import detect_code_language
from cocoindex.resources.file import FileLike, PatternFilePathMatcher

from pipelines.shared.config import TARGET_SQLITE_PATH, EXCLUDE_PATTERNS

SQLITE_DB = coco.ContextKey[sqlite.ManagedConnection]("app_db")


@dataclass
class CodeSymbol:
    name: str
    kind: str
    file_path: str
    line: int
    signature: str
    language: str


# Symbol patterns by language
SYMBOL_PATTERNS = {
    "rust": [
        (r"(?:pub\s+)?(?:async\s+)?fn\s+(\w+)", "function"),
        (r"(?:pub\s+)?struct\s+(\w+)", "struct"),
        (r"(?:pub\s+)?enum\s+(\w+)", "enum"),
        (r"(?:pub\s+)?trait\s+(\w+)", "trait"),
        (r"(?:pub\s+)?impl\s+(?:<[^>]+>\s+)?(\w+)", "impl"),
        (r"(?:pub\s+)?type\s+(\w+)", "type"),
        (r"(?:pub\s+)?mod\s+(\w+)", "module"),
        (r"macro_rules!\s*(\w+)", "macro"),
    ],
    "python": [
        (r"(?:def|async\s+def)\s+(\w+)", "function"),
        (r"(?:class\s+)(\w+)", "class"),
    ],
    "typescript": [
        (r"(?:export\s+)?(?:async\s+)?function\s+(\w+)", "function"),
        (r"(?:export\s+)?class\s+(\w+)", "class"),
        (r"(?:export\s+)?interface\s+(\w+)", "interface"),
    ],
}


@coco.lifespan
def metadata_lifespan(builder: coco.EnvironmentBuilder) -> Iterator[None]:
    with sqlite.managed_connection(str(TARGET_SQLITE_PATH)) as conn:
        builder.provide(SQLITE_DB, conn)
        yield


@coco.fn(memo=True)
async def process_file(file: FileLike) -> None:
    """Extract metadata from a single file."""
    content = await file.read_text()
    path = str(file.file_path.path)
    language = detect_code_language(filename=path)

    patterns = SYMBOL_PATTERNS.get(language, [])
    symbols_to_declare = []

    for line_num, line in enumerate(content.split("\n"), 1):
        for pattern, kind in patterns:
            match = re.search(pattern, line)
            if match:
                name = match.group(1)
                if len(name) >= 2 and name[0].islower():
                    continue
                symbols_to_declare.append(CodeSymbol(
                    name=name,
                    kind=kind,
                    file_path=path,
                    line=line_num,
                    signature=line.strip()[:100],
                    language=language,
                ))

    # Mount table to store symbols
    table = await sqlite.mount_table_target(
        SQLITE_DB,
        "code_symbols",
        await sqlite.TableSchema.from_class(CodeSymbol, primary_key=["name", "kind", "file_path"]),
    )

    # Deduplicate before declaring
    seen = set()
    for sym in symbols_to_declare:
        key = (sym.name, sym.kind, sym.file_path)
        if key not in seen:
            seen.add(key)
            table.declare_row(row=sym)


@coco.fn
async def app_main() -> None:
    """Main entry point for metadata extraction."""
    files = localfs.walk_dir(
        pathlib.Path("."),
        recursive=True,
        path_matcher=PatternFilePathMatcher(
            included_patterns=["**/*.rs", "**/*.py", "**/*.ts", "**/*.tsx"],
            excluded_patterns=EXCLUDE_PATTERNS,
        ),
        live=True,
    )

    await coco.mount_each(process_file, files.items())


app = coco.App(
    coco.AppConfig(name="metadata-extraction"),
    app_main,
)