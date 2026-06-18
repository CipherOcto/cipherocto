"""Test indexer - indexes test files separately for test-aware search."""

from dataclasses import dataclass
from typing import Iterator, AsyncIterator
from pathlib import Path

import cocoindex as coco
from cocoindex.connectors import localfs, sqlite

from pipelines.shared.config import TARGET_SQLITE_PATH, PROJECT_ROOT, INCLUDE_PATTERNS, EXCLUDE_PATTERNS

SQLITE_DB = coco.ContextKey[sqlite.ManagedConnection]("app_db")

# Test file patterns
TEST_PATTERNS = [
    "**/test_*.py",
    "**/*_test.py",
    "**/tests/**/*.py",
    "**/test/**/*.py",
    "**/__tests__/**/*.py",
    "**/*.spec.ts",
    "**/*.test.ts",
    "**/*.test.js",
    "**/tests/*.rs",
]


@dataclass
class TestFile:
    """Index entry for a test file."""
    file_path: str
    test_name: str  # Extracted test function/method name
    test_type: str  # "unit", "integration", "e2e"
    framework: str  # "pytest", "unittest", "jest", "rust"
    line_number: int
    content: str


def detect_test_framework(filename: str) -> str | None:
    """Detect test framework from file patterns."""
    name = str(filename).lower()

    if "pytest" in name or name.startswith("test_") or name.endswith("_test.py"):
        return "pytest"
    if name.endswith("_test.py"):
        return "unittest"
    if ".test." in name or ".spec." in name:
        return "jest"
    if filename.endswith(".rs") and "test" in name:
        return "rust"
    return None


def parse_test_functions(content: str, filename: str) -> list[TestFile]:
    """Parse test functions from file content."""
    tests = []
    framework = detect_test_framework(filename)
    if not framework:
        return []

    # Python test patterns
    python_patterns = [
        r"def\s+(test_[a-zA-Z0-9_]+)\s*\(",
        r"def\s+([a-zA-Z0-9_]+Test)\s*\(",
        r"async\s+def\s+(test_[a-zA-Z0-9_]+)\s*\(",
    ]

    # JS/TS test patterns
    js_patterns = [
        r"(?:test|it)\s*\(\s*['\"]([^'\"]+)['\"]",
        r"describe\s*\(\s*['\"]([^'\"]+)['\"]",
    ]

    # Rust test patterns
    rust_patterns = [
        r"#\[test\]",
        r"#\[cfg\(test\)\]",
        r"fn\s+(test_[a-zA-Z0-9_]+)",
    ]

    for line_num, line in enumerate(content.split("\n"), 1):
        patterns = []
        if framework in ("pytest", "unittest"):
            patterns = python_patterns
        elif framework == "jest":
            patterns = js_patterns
        elif framework == "rust":
            patterns = rust_patterns

        for pattern in patterns:
            import re
            match = re.search(pattern, line)
            if match:
                test_name = match.group(1) if match.lastindex else "unnamed_test"

                # Determine test type
                test_type = "unit"
                if "integration" in line.lower() or "integration_test" in str(filename):
                    test_type = "integration"
                elif "e2e" in line.lower() or "end_to_end" in str(filename):
                    test_type = "e2e"

                tests.append(TestFile(
                    file_path=str(filename),
                    test_name=test_name,
                    test_type=test_type,
                    framework=framework,
                    line_number=line_num,
                    content=line.strip(),
                ))
                break

    return tests


@coco.lifespan
def test_indexer_lifespan(builder: coco.EnvironmentBuilder) -> Iterator[None]:
    with sqlite.managed_connection(str(TARGET_SQLITE_PATH)) as conn:
        builder.provide(SQLITE_DB, conn)
        yield


@coco.fn
async def app_main() -> None:
    """Index test files separately."""
    source_dir = PROJECT_ROOT

    # Find all test files
    test_files = []
    for pattern in TEST_PATTERNS:
        test_files.extend(source_dir.glob(pattern))

    # Filter excluded directories
    excluded_dirs = {"node_modules", "target", "vendor", ".git", ".venv"}
    test_files = [f for f in test_files if not any(part in f.parts for part in excluded_dirs)]

    print(f"Indexing {len(test_files)} test files...")

    # Mount the test_index table
    schema = await sqlite.TableSchema.from_class(TestFile, primary_key=["file_path", "test_name", "line_number"])
    table = await sqlite.mount_table_target(SQLITE_DB, "test_index", schema)

    indexed = 0
    for filepath in test_files:
        try:
            content = filepath.read_text(encoding="utf-8", errors="ignore")
            tests = parse_test_functions(content, filepath)

            for test in tests:
                table.declare_row(row=test)

            if tests:
                indexed += 1
        except Exception as e:
            continue

    print(f"Indexed {indexed} test files")


app = coco.App(
    coco.AppConfig(name="test-indexer"),
    app_main,
)