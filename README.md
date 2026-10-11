# Kurobayomi extensions

Sources for [Kurobayomi](https://github.com/anas1412/kurobayomi). One folder per source, `sources/<id>/`. A source's kind
(manga or anime) is a field of its file, not a folder. A source is either a text recipe or a WASM plug-in.

Sources are ported from open-source Aniyomi and Mihon extension repositories (Apache-2.0; see `LICENSE` and `NOTICE`). Each source's README names the
original and what was changed.

## Layout

```
sources/<id>/         one folder per source
  manifest.toml       a WASM plug-in: id, name, lang, kind, version, hosts
  Cargo.toml, src/    the plug-in crate
  recipe.toml         or a text recipe: id, name, lang, kind, base_urls, selectors
  fixtures/           saved pages or responses the tests use
  README.md           what the source does, its licence, what was changed
parked/<id>/          sources kept but not offered (not read by the index tool)
template/             starter crate for a new plug-in: copy it to sources/<id>/
dist/                 built files the index links to: <id>.wasm and <id>.toml (committed)
tools/build.sh        builds every plug-in under sources/ into dist/
tools/make-index.py   copies recipes into dist/ and writes index.min.json
index.min.json        generated, never edit by hand
NEEDS-ENGINE.md       engine changes that the recipes need
LICENSE, NOTICE       Apache License 2.0 text and the notice for the ported sources and the files in dist/
```

## How to add this repository to the app

**Not working in the current app.** The app's add-link step (`extension_store::add_store`) reads a legacy repo (`meta`) or a
store (`name`, `badgeLabel`, `signingKey`, `contact`), not this index (`{"version":1,"sources":[...]}`). The only parser for
this format, `crates/kuro-extensions/src/repo.rs`, is not called anywhere yet. Which format the app reads is open: either
this tool emits the store format or the app wires `repo.rs` in. The steps below describe the intended flow.

Also open (app side): the app does not read this repo's `manifest.toml`. `kuro-source-wasm` takes `hosts` from a `manifest.json`
inside a `.kuroext` zip, and `kuro-extensions` (folder with `manifest.toml` + `component.wasm`, no `hosts` field) passes an empty
list to `instantiate`, so every `http` call would be refused. Until the app picks one package format and carries `hosts` through, a
plug-in from this index cannot fetch. The packaging step must then derive that format (pkg, versionName, versionCode, abi, baseUrl,
source id) from `manifest.toml` and the index entry. Also not wired in the app yet: `scan_and_instantiate` is never called from `src/`.

Kurobayomi ships with no online sources. You add a repository by its link, and its sources appear at once. There is no
install step.

1. In Kurobayomi open **Browse**, then the menu, then **Extension repos**, and add this link:
   `https://raw.githubusercontent.com/anas1412/kurobayomi-extensions/main/index.min.json`
2. Browse lists the sources of the repository, grouped by language, with a language filter.
3. A source's file is downloaded the first time you open that source, and its checksum is checked against the index.
   The daily update check refreshes the index and the files.
4. To remove the repository, remove its link in **Extension repos**; its sources go with it.

A repository lists at most 10 sources of each kind (manga and anime).

Developers can test a recipe without a repository: copy `sources/<id>/recipe.toml` to
`~/.local/share/kurobayomi/scrapers/<id>.toml` and restart the app.

## Add a source

### A text recipe

1. Create `sources/<id>/recipe.toml`. Its `id` must equal the folder name. It carries its own `id`, `name`, `lang` and
   `kind` (`manga` or `anime`), and an optional `version` (default 1).
2. Add `fixtures/` with saved pages, and a `README.md` that names the original and its licence if the recipe is ported.
3. Run `python3 tools/make-index.py`.

### A WASM plug-in

1. Copy the starter: `cp -r template sources/<id>`. Rename the package in `Cargo.toml` and write the ops in `src/lib.rs`
   against `wit/kuro-source.wit` (ABI 1.4).
2. Create `sources/<id>/manifest.toml`:

   ```toml
   id = "example"        # must equal the folder name
   name = "Example"
   lang = "en"
   kind = "manga"        # or "anime"
   version = 1
   hosts = ["example.org"]   # the hosts the plug-in may call
   ```

3. Add `fixtures/` and a `README.md`, as for a recipe.
4. Run `tools/build.sh`, then `python3 tools/make-index.py`.

The plug-in calls the network only through the host's `http` import. The plug-in itself is never listed unless its file is
in `dist/`.

## Build and index

Once: `rustup target add wasm32-unknown-unknown`, and `wasm-component-ld` must be on the path.

Plug-ins are built for `wasm32-unknown-unknown` and componentized by `wasm-component-ld`, not for `wasm32-wasip2`. A wasip2
build imports `wasi:cli/*`, `wasi:io/*` and `wasi:clocks/*`, and the app's sandbox has no WASI (it links only
`kuro:source/host`), so it could not start. `tools/build.sh` and `tools/make-index.py` reject any `.wasm` that names a
`wasi:` interface.

A plug-in must stay under 1 MiB (the app's limit). The starter profile sets `opt-level = "s"`, `lto`, `codegen-units = 1`,
`panic = "abort"` and `strip`; record each plug-in's size in its README.

- `tools/build.sh` builds each plug-in folder with `cargo build --release --target wasm32-unknown-unknown -j 1`, one at a time, and
  copies the result to `dist/<id>.wasm`. Builds share `target/` at the repository root, which git ignores.
- `tools/make-index.py` reads `sources/*/`, copies each recipe to `dist/<id>.toml`, checks plug-ins, deletes files in `dist/` that no entry names, and writes `index.min.json`. A plug-in
  whose `dist/<id>.wasm` does not exist yet is skipped with a warning.

Each index entry has `id`, `name`, `lang`, `kind`, `version`, `file` (the raw https link of the file in `dist/`), `sha256`
(of that file's bytes) and `type` (`wasm` or `recipe`). The app stores the copy as `<id>.wasm` or `<id>.toml`, by the
extension of the link. A file is at most 1 MiB.

Commit `dist/` and `index.min.json` together after a change, because the checksums must match the files on the branch.

## Sources

Status as of 2026-10-11.

| Source | Kind | Lang | Type | Status |
|---|---|---|---|---|
| `existentialcomics` | manga | en | recipe | works |
| `gwtb` (Gone with the Blastwave) | manga | en | recipe | recipe done; its chapter links needed an engine fix |
| `beatzanime` | anime | es | recipe | browse, details and episode list work; search and playback do not yet (see NEEDS-ENGINE.md) |
| `parked/kimoitv` | anime | en | parked, not offered | no playback: the site's Cloudflare Turnstile bot check blocks it |
| `weebcentral` | manga | en | WASM plug-in | `dist/weebcentral.wasm` (942 KB, no WASI imports); not run in the app yet |
| `comix` | manga | en | WASM plug-in | not built yet, not in the index; the Rust version works against the live site |
| `kickassanime` | anime | en | WASM plug-in | not built yet, not in the index; the Rust version plays HLS with subtitles and audio tracks |
| `parked/animepahe` | anime | en | parked, not offered | the site's Cloudflare challenge cannot be passed by a sandboxed plug-in |

The plug-ins are not verified against the live sites from inside the app's sandbox host.
