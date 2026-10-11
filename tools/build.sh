#!/usr/bin/env bash
# Builds every WASM plug-in under sources/ into dist/<id>.wasm, one crate at a time (-j 1: the machine has little memory).
# Needs the target once: rustup target add wasm32-unknown-unknown, and wasm-component-ld (it ships with rustc's wasip2 support).
# Plug-ins are built for wasm32-unknown-unknown and componentized by wasm-component-ld, NOT for wasm32-wasip2: a wasip2
# build imports wasi:cli/* and wasi:io/*, and the app's sandbox links only kuro:source/host, so it could not instantiate.
# Each result is checked to import nothing but kuro:source/host. Run tools/make-index.py afterwards to refresh index.min.json.
set -euo pipefail
shopt -s nullglob

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
target="${CARGO_TARGET_DIR:-$root/target}"
mkdir -p "$root/dist"

built=0
for crate in "$root"/sources/*/; do
    id="$(basename "$crate")"
    [ -f "$crate/Cargo.toml" ] || continue
    if [ ! -f "$crate/manifest.toml" ]; then
        echo "build: sources/$id has a Cargo.toml but no manifest.toml, skipped" >&2
        continue
    fi
    # the file cargo writes is named after the library, which is the package name with '-' turned into '_'
    lib_name="$(python3 -c '
import sys, tomllib
data = tomllib.load(open(sys.argv[1], "rb"))
print((data.get("lib", {}).get("name") or data["package"]["name"]).replace("-", "_"))
' "$crate/Cargo.toml")"
    echo "build: $id"
    (cd "$crate" && CARGO_TARGET_DIR="$target" CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER=wasm-component-ld \
        cargo build --release --target wasm32-unknown-unknown -j 1)
    out="$target/wasm32-unknown-unknown/release/$lib_name.wasm"
    python3 "$root/tools/make-index.py" --check-wasm "$out"
    size="$(stat -c %s "$out")"
    if [ "$size" -gt 1048576 ]; then echo "build: $id is $size bytes, over the 1 MiB limit" >&2; exit 1; fi
    cp "$out" "$root/dist/$id.wasm"
    built=$((built + 1))
done
echo "built $built plug-in(s) into dist/"
