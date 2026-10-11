#!/usr/bin/env python3
"""Writes index.min.json: the list of sources this repository offers.

Each source is a folder manga/<name>/ or anime/<name>/ holding a recipe.toml. Folders whose name starts with "_" are
parked and skipped. The index lists, for each source: id, name, lang, kind, version, file (the raw https link of its
recipe.toml) and sha256 (of the file's bytes). The app fetches a file only when a user first opens its source, and checks
the bytes against sha256. A repository lists at most 10 sources of each kind.

Usage: python3 tools/make-index.py [--base-url URL] [--out PATH]
"""

import argparse
import hashlib
import json
import re
import sys
import tomllib
from pathlib import Path

DEFAULT_BASE_URL = "https://raw.githubusercontent.com/anas1412/kurobayomi-extensions/main"
KINDS = ("manga", "anime")
LIMIT_PER_KIND = 10
ID_PATTERN = re.compile(r"[a-z0-9._-]+")


def read_sources(root):
    for kind in KINDS:
        kind_dir = root / kind
        if not kind_dir.is_dir():
            continue
        for folder in sorted(kind_dir.iterdir()):
            if not folder.is_dir() or folder.name.startswith("_"):
                continue
            recipe_path = folder / "recipe.toml"
            if not recipe_path.is_file():
                sys.exit(f"{folder}: no recipe.toml")
            data = recipe_path.read_bytes()
            recipe = tomllib.loads(data.decode("utf-8"))
            if recipe.get("kind") != kind:
                sys.exit(f"{recipe_path}: kind is {recipe.get('kind')!r}, but the folder is {kind!r}")
            if recipe.get("id") != folder.name or not ID_PATTERN.fullmatch(folder.name):
                sys.exit(f"{recipe_path}: id must be the folder name, lower-case letters, digits, '.', '_' or '-'")
            yield kind, recipe_path, data, recipe


def build_index(root, base_url):
    entries = []
    for kind, recipe_path, data, recipe in read_sources(root):
        entries.append({
            "id": recipe["id"],
            "name": recipe["name"],
            "lang": recipe["lang"],
            "kind": kind,
            "version": int(recipe.get("version", 1)),
            "file": f"{base_url}/{recipe_path.relative_to(root).as_posix()}",
            "sha256": hashlib.sha256(data).hexdigest(),
        })
    for kind in KINDS:
        count = sum(1 for entry in entries if entry["kind"] == kind)
        if count > LIMIT_PER_KIND:
            sys.exit(f"{count} {kind} sources; a repository lists at most {LIMIT_PER_KIND} of each kind")
    return {"version": 1, "sources": entries}


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--base-url", default=DEFAULT_BASE_URL, help="raw https link of the repository root")
    parser.add_argument("--out", type=Path, default=None, help="output file (default: index.min.json at the root)")
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    out = args.out or root / "index.min.json"
    index = build_index(root, args.base_url.rstrip("/"))
    out.write_text(json.dumps(index, ensure_ascii=False, separators=(",", ":")) + "\n", encoding="utf-8")
    print(f"wrote {out}: {len(index['sources'])} sources")


if __name__ == "__main__":
    main()
