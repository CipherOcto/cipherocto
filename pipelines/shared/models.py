"""Shared data models for indexing pipelines."""

from dataclasses import dataclass
from typing import Optional


@dataclass
class CodeChunk:
    """A chunk of code indexed in SQLite."""
    id: int
    filename: str
    code: str
    start_line: int
    end_line: int
    language: Optional[str] = None


@dataclass
class CodeSymbol:
    """An extracted code symbol (function, class, etc.)."""
    id: int
    name: str
    kind: str
    file_path: str
    line: int
    signature: str
    docstring: Optional[str] = None


@dataclass
class FileSummary:
    """Summary metadata for a file."""
    path: str
    file_type: str
    size_bytes: int
    line_count: int
    imports: list
    exports: list
    doc_summary: str