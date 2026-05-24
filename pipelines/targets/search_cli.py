#!/usr/bin/env python3
"""Semantic code search CLI — search code chunks and/or symbols."""

import argparse
import json
import sys

from pipelines.shared.utils import search_chunks, search_symbols, search_unified


def main():
    parser = argparse.ArgumentParser(description="Semantic code search")
    parser.add_argument("query", help="Search query")
    parser.add_argument("--db", default=None, help="Database path (default from config)")
    parser.add_argument("-k", "--top-k", type=int, default=10, help="Number of results")
    parser.add_argument("-t", "--threshold", type=float, default=0.25, help="Similarity threshold")
    parser.add_argument("--chunks", action="store_true", help="Search code chunks only")
    parser.add_argument("--symbols", action="store_true", help="Search symbols only")
    parser.add_argument("--json", action="store_true", help="Output as JSON")
    args = parser.parse_args()

    kwargs = {"query": args.query, "top_k": args.top_k, "threshold": args.threshold}
    if args.db:
        kwargs["db_path"] = args.db

    if args.symbols:
        results = search_symbols(**kwargs)
    elif args.chunks:
        results = search_chunks(**kwargs)
    else:
        results = search_unified(**kwargs)

    if not results:
        print("No results found")
        return

    if args.json:
        print(json.dumps(results, indent=2))
        return

    for i, r in enumerate(results, 1):
        score = r["score"]
        if r["type"] == "chunk":
            loc = f"{r['filename']}:{r['start_line']}-{r['end_line']}"
            preview = r["text"][:200].replace("\n", " ")
            print(f"\n[{score:.3f}] chunk  {loc}")
            print(f"    {preview}")
        else:
            sig = r["signature"][:100]
            print(f"\n[{score:.3f}] {r['kind']} {r['name']}")
            print(f"    {sig}")
            print(f"    -> {r['file_path']}:{r['line']}")


if __name__ == "__main__":
    main()
