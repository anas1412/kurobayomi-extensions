#!/usr/bin/env python3
"""Writes index.min.json: the sources this repository offers.

Each source is one folder, sources/<id>/, and its kind comes from the file it holds, not from where it sits:
- manifest.toml: a WASM plug-in. id, name, lang, kind and version are read from it. Its file is dist/<id>.wasm, which
  tools/build.sh makes. A plug-in whose dist file does not exist yet is skipped with a warning.
- recipe.toml: a text recipe. id, name, lang, kind and version (default 1) are read from it. Its file is copied to
  dist/<id>.toml here, so the index always matches the recipe.

parked/ is not read. Each entry has id, name, lang, kind, version, file (the raw https link of the dist file), sha256 (of
the file's bytes) and type ("wasm" or "recipe"). The app fetches a file only when a user first opens its source, and
checks the bytes against sha256. A repository lists at most 10 sources of each kind.

Usage: python3 tools/make-index.py [--base-url URL] [--out PATH]
"""

import argparse
import hashlib
import json
import re
import shutil
import sys
import tomllib
from pathlib import Path

DEFAULT_BASE_URL = "https://raw.githubusercontent.com/anas1412/kurobayomi-extensions/main"
KINDS = ("manga", "anime")
LIMIT_PER_KIND = 10
MAX_FILE_BYTES = 1024 * 1024  # the app refuses a larger file
ID_PATTERN = re.compile(r"[a-z0-9._-]+")


def fail(message):
    sys.exit(f"error: {message}")


def check_wasm(path):
    """The app's sandbox has no WASI: it links only kuro:source/host. Reject a component that names anything else.

    ponytail: a byte scan, not a parse (no wasm-tools here). It rejects any 'wasi:' name anywhere in the file, which is
    stricter than the imports alone, and needs the host import name to be present.
    """
    data = Path(path).read_bytes()
    if not data.startswith(b"\0asm\x0d\x00\x01\x00"):
        fail(f"{path}: not a WASM component (build for wasm32-unknown-unknown and componentize; see tools/build.sh)")
    if b"wasi:" in data:
        fail(f"{path}: names a wasi: interface; the app's sandbox has no WASI. Build with tools/build.sh, not wasm32-wasip2")
    if b"kuro:source/host" not in data:
        fail(f"{path}: does not import kuro:source/host")


def read_meta(folder):
    """(type, path, metadata) of a source folder, or None when it holds neither a manifest nor a recipe."""
    has_manifest = (folder / "manifest.toml").is_file()
    has_recipe = (folder / "recipe.toml").is_file()
    if has_manifest and has_recipe:
        fail(f"{folder}: has both manifest.toml and recipe.toml; a source is one or the other")
    if not (has_manifest or has_recipe):
        return None
    kind_type, path = ("wasm", folder / "manifest.toml") if has_manifest else ("recipe", folder / "recipe.toml")
    try:
        return kind_type, path, tomllib.loads(path.read_text(encoding="utf-8"))
    except tomllib.TOMLDecodeError as error:
        fail(f"{path}: {error}")


def build_index(root, base_url):
    dist = root / "dist"
    sources = root / "sources"
    entries = []
    for folder in sorted(sources.iterdir()) if sources.is_dir() else []:
        if not folder.is_dir():
            continue
        found = read_meta(folder)
        if found is None:
            print(f"warning: sources/{folder.name} has no manifest.toml or recipe.toml, skipped", file=sys.stderr)
            continue
        kind_type, meta_path, meta = found
        if meta.get("id") != folder.name or not ID_PATTERN.fullmatch(folder.name):
            fail(f"{meta_path}: id must be the folder name {folder.name!r} (lower-case letters, digits, '.', '_', '-')")
        if meta.get("kind") not in KINDS:
            fail(f"{meta_path}: kind must be 'manga' or 'anime', not {meta.get('kind')!r}")
        name, lang = meta.get("name"), meta.get("lang")
        if not (isinstance(name, str) and name and isinstance(lang, str) and lang):
            fail(f"{meta_path}: name and lang must be non-empty text")
        version = meta.get("version", 1)
        if not (isinstance(version, int) and not isinstance(version, bool) and version >= 1):
            fail(f"{meta_path}: version must be a whole number from 1")

        if kind_type == "wasm":
            hosts = meta.get("hosts")
            if not (isinstance(hosts, list) and hosts and all(isinstance(h, str) and h and "/" not in h for h in hosts)):
                fail(f"{meta_path}: hosts must be a non-empty list of host names such as 'example.org'")
            built = dist / f"{folder.name}.wasm"
            if not built.is_file():
                print(f"warning: {built.relative_to(root)} is not built, sources/{folder.name} skipped (run tools/build.sh)",
                      file=sys.stderr)
                continue
        else:
            dist.mkdir(exist_ok=True)
            built = dist / f"{folder.name}.toml"
            shutil.copyfile(meta_path, built)

        if kind_type == "wasm":
            check_wasm(built)
        data = built.read_bytes()
        if len(data) > MAX_FILE_BYTES:
            fail(f"{built}: {len(data)} bytes, the app accepts at most {MAX_FILE_BYTES}")
        entries.append({
            "id": folder.name,
            "name": name,
            "lang": lang,
            "kind": meta["kind"],
            "version": version,
            "file": f"{base_url}/{built.relative_to(root).as_posix()}",
            "sha256": hashlib.sha256(data).hexdigest(),
            "type": kind_type,
        })
        if kind_type == "wasm":
            entries[-1]["hosts"] = hosts
    for kind in KINDS:
        count = sum(1 for entry in entries if entry["kind"] == kind)
        if count > LIMIT_PER_KIND:
            fail(f"{count} {kind} sources; a repository lists at most {LIMIT_PER_KIND} of each kind")
    # dist/ holds only what the index names: a source moved to parked/ or removed must not leave its file behind.
    named = {Path(entry["file"]).name for entry in entries}
    for stale in sorted(dist.iterdir()) if dist.is_dir() else []:
        if stale.is_file() and stale.name not in named:
            print(f"removing stale dist/{stale.name}", file=sys.stderr)
            stale.unlink()
    return {"version": 1, "sources": entries}


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--base-url", default=DEFAULT_BASE_URL, help="raw https link of the repository root")
    parser.add_argument("--check-wasm", type=Path, default=None, help="only check that a built .wasm imports nothing but kuro:source/host")
    parser.add_argument("--out", type=Path, default=None, help="output file (default: index.min.json at the root)")
    args = parser.parse_args()
    if args.check_wasm:
        check_wasm(args.check_wasm)
        return
    root = Path(__file__).resolve().parent.parent
    out = args.out or root / "index.min.json"
    index = build_index(root, args.base_url.rstrip("/"))
    out.write_text(json.dumps(index, ensure_ascii=False, separators=(",", ":")) + "\n", encoding="utf-8")
    plugins = sum(1 for entry in index["sources"] if entry["type"] == "wasm")
    print(f"wrote {out}: {len(index['sources'])} sources ({plugins} plug-ins, {len(index['sources']) - plugins} recipes)")


if __name__ == "__main__":
    main()
