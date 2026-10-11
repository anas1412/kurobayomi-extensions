#!/usr/bin/env python3
"""Writes dist/<id>.kuroext (manifest.json + source.wasm in a zip) for each plug-in that has a dist/<id>.wasm.

The package is UNSIGNED: the app's installer also wants an ed25519 signature and a repo index entry (see
kuro-source-wasm/src/verify.rs); how to sign and index is the lead's decision. Needs the extra manifest.toml fields
pkg, version_name, version_code, abi, base_url. Usage: python3 tools/package.py [id ...]
"""
import hashlib, json, sys, tomllib, zipfile
from pathlib import Path

root = Path(__file__).resolve().parent.parent


def source_id(name, lang, version=1):
    """kuro_source::generate_id: first 8 bytes of MD5 of 'name/lang/version' (name lower-cased), masked to i64."""
    digest = hashlib.md5(f"{name.lower()}/{lang}/{version}".encode()).digest()
    return int.from_bytes(digest[:8], "big") & 0x7FFFFFFFFFFFFFFF


for folder in sorted((root / "sources").iterdir()):
    if len(sys.argv) > 1 and folder.name not in sys.argv[1:]:
        continue
    path, wasm = folder / "manifest.toml", root / "dist" / f"{folder.name}.wasm"
    if not (path.is_file() and wasm.is_file()):
        continue
    m = tomllib.loads(path.read_text(encoding="utf-8"))
    manifest = {
        "schema": 1, "pkg": m["pkg"], "name": m["name"], "versionName": m["version_name"],
        "versionCode": m["version_code"], "abi": m["abi"], "lang": m["lang"], "hosts": m["hosts"],
        "sources": [{"id": source_id(m["name"], m["lang"]), "lang": m["lang"], "name": m["name"], "baseUrl": m["base_url"]}],
    }
    out = root / "dist" / f"{folder.name}.kuroext"
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("manifest.json", json.dumps(manifest, indent=2))
        z.write(wasm, "source.wasm")
    print(f"wrote {out}")
