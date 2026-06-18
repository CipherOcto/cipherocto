"""API endpoint extractor - indexes HTTP endpoints for API documentation."""

from dataclasses import dataclass
from typing import Iterator
import re
from pathlib import Path

import cocoindex as coco
from cocoindex.connectors import localfs, sqlite

from pipelines.shared.config import TARGET_SQLITE_PATH, PROJECT_ROOT

SQLITE_DB = coco.ContextKey[sqlite.ManagedConnection]("app_db")


@dataclass
class ApiEndpoint:
    """Index entry for an API endpoint."""
    file_path: str
    method: str  # GET, POST, PUT, DELETE, PATCH, etc.
    path: str
    handler: str  # Function name that handles this endpoint
    line_number: int


# Common web framework patterns
ENDPOINT_PATTERNS = {
    # FastAPI / Flask / Starlette
    "python": [
        (r'@(?:app|router)\.(get|post|put|delete|patch|options|head)\s*\(\s*["\']([^"\']+)["\']', "decorator"),
        (r'@(?:app|router)\.(get|post|put|delete|patch|options|head)\s*\([^)]+\)', "decorator"),
        # Django
        (r'@(?:path|route)\s*\(\s*["\']([^"\']+)["\']', "django"),
        # Flask routes
        (r'@app\.route\s*\(\s*["\']([^"\']+)["\']', "flask"),
        # Pyramid
        (r'@(?:view_config|config\.add_route)\([^)]*route_name=["\']([^"\']+)["\']', "pyramid"),
    ],
    # Express / Node.js
    "javascript": [
        (r'(?:app|router)\.(get|post|put|delete|patch|options)\s*\(\s*["\']([^"\']+)["\']', "express"),
        (r'express\s*\(\s*\).*(?:get|post|put|delete|patch)\s*\(\s*["\']([^"\']+)["\']', "express"),
        # Next.js API routes
        (r'export\s+(?:async\s+)?function\s+(?:GET|POST|PUT|DELETE|PATCH)\s*\(', "nextjs"),
    ],
    # Actix-web (Rust)
    "rust": [
        (r'#\[actix_web::(?:get|post|put|delete|patch|head|options)\s*\("([^"]+)"', "actix"),
        (r'#\[get\s*\("([^"]+)"', "actix"),
        (r'#\[post\s*\("([^"]+)"', "actix"),
        (r'#\[put\s*\("([^"]+)"', "actix"),
        (r'#\[delete\s*\("([^"]+)"', "actix"),
        (r'#\[patch\s*\("([^"]+)"', "actix"),
        # Axum
        (r'async\s+fn\s+(\w+)\s*\(.*route\s*\(\s*["\']([^"\']+)["\']', "axum"),
    ],
    # Go (net/http, gin, echo)
    "go": [
        (r'(?:http\.|r\.)?(?:HandleFunc|Get|Post|Put|Delete|Patch)\s*\(\s*["\']([^"\']+)["\']', "go-std"),
        (r'router\.(?:GET|POST|PUT|DELETE|PATCH)\s*\(\s*["\']([^"\']+)["\']', "go-gin"),
        (r'echo\.(?:GET|POST|PUT|DELETE|PATCH)\s*\(\s*["\']([^"\']+)["\']', "go-echo"),
    ],
}


def detect_framework(filename: str, content: str) -> str | None:
    """Detect web framework from file content."""
    name = str(filename).lower()
    content_lower = content.lower()

    if "fastapi" in content_lower or "from fastapi import" in content_lower:
        return "fastapi"
    if "flask" in content_lower or "from flask import" in content_lower:
        return "flask"
    if "django" in content_lower or "from django" in content_lower:
        return "django"
    if "express" in content_lower or "require('express')" in content_lower:
        return "express"
    if "next" in name and ("page" in name or "route" in name):
        return "nextjs"
    if "actix" in content_lower or "actix-web" in content_lower:
        return "actix"
    if filename.endswith(".go"):
        return "go"
    return None


def parse_endpoints(content: str, filename: str) -> list[ApiEndpoint]:
    """Parse API endpoints from file content."""
    endpoints = []
    framework = detect_framework(filename, content)

    if not framework:
        return endpoints

    lines = content.split("\n")

    for line_num, line in enumerate(lines, 1):
        line = line.strip()

        # Determine language
        lang = None
        if filename.endswith(".py"):
            lang = "python"
        elif filename.endswith((".js", ".ts", ".jsx", ".tsx")):
            lang = "javascript"
        elif filename.endswith(".rs"):
            lang = "rust"
        elif filename.endswith(".go"):
            lang = "go"

        if not lang:
            continue

        patterns = ENDPOINT_PATTERNS.get(lang, [])

        for pattern, ptype in patterns:
            match = re.search(pattern, line)
            if match:
                # Extract method and path
                if ptype in ("decorator", "express", "actix", "django", "flask", "go-std", "go-gin", "go-echo"):
                    method = match.group(1).upper() if match.lastindex >= 1 else "GET"
                    path = match.group(2) if match.lastindex >= 2 else match.group(1) if match.lastindex >= 1 else "/"
                    # Clean up path
                    path = path.strip("'\"")

                    # Find handler function (look ahead for async def/def)
                    handler = "unknown"
                    for look_ahead in range(line_num, min(line_num + 10, len(lines))):
                        fn_match = re.search(r'(?:async\s+)?def\s+(\w+)', lines[look_ahead])
                        if fn_match:
                            handler = fn_match.group(1)
                            break

                    endpoints.append(ApiEndpoint(
                        file_path=str(filename),
                        method=method,
                        path=path,
                        handler=handler,
                        line_number=line_num,
                    ))
                break

    return endpoints


@coco.lifespan
def api_extractor_lifespan(builder: coco.EnvironmentBuilder) -> Iterator[None]:
    with sqlite.managed_connection(str(TARGET_SQLITE_PATH)) as conn:
        builder.provide(SQLITE_DB, conn)
        yield


@coco.fn
async def app_main() -> None:
    """Index API endpoints across the codebase."""
    source_dir = PROJECT_ROOT

    # Get all web/API files
    api_files = []
    for ext in [".py", ".js", ".ts", ".rs", ".go"]:
        api_files.extend(source_dir.rglob(f"*{ext}"))

    # Filter excluded directories
    excluded_dirs = {"node_modules", "target", "vendor", ".git", ".venv"}
    api_files = [f for f in api_files if not any(part in f.parts for part in excluded_dirs)]

    print(f"Indexing {len(api_files)} files for API endpoints...")

    # Mount the api_endpoints table
    schema = await sqlite.TableSchema.from_class(ApiEndpoint, primary_key=["file_path", "method", "path", "line_number"])
    table = await sqlite.mount_table_target(SQLITE_DB, "api_endpoints", schema)

    indexed = 0
    for filepath in api_files:
        try:
            content = filepath.read_text(encoding="utf-8", errors="ignore")
            endpoints = parse_endpoints(content, filepath)

            for endpoint in endpoints:
                table.declare_row(row=endpoint)

            if endpoints:
                indexed += 1
        except Exception as e:
            continue

    print(f"Indexed endpoints from {indexed} files")


app = coco.App(
    coco.AppConfig(name="api-extractor"),
    app_main,
)