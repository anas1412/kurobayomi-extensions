# Kurobayomi extensions

Sources for [Kurobayomi](https://github.com/anas1412/kurobayomi), the anime and manga app for the Linux desktop.
Kurobayomi ships with no sources: you add a repository like this one by its link, and its sources appear in the app at
once. There is no install step.

Every source here is a port of an open-source Aniyomi or Mihon extension (Apache-2.0; see [LICENSE](LICENSE) and
[NOTICE](NOTICE)). Each source's own README names the original and what was changed.

## Add this repository to Kurobayomi

1. Open Kurobayomi and click **Add a source repository** (or **Browse → menu → Repositories → Add**).
2. Paste this link and click **Add**:

   ```
   https://raw.githubusercontent.com/anas1412/kurobayomi-extensions/main/index.min.json
   ```

3. The sources show up in **Browse** right away, grouped by language.

The app checks the repository once a day and picks up fixes and new sources on its own. To remove the repository,
remove its link under **Repositories**; its sources go with it. A step-by-step guide with screenshots is in the app's
[Getting started](https://github.com/anas1412/kurobayomi/blob/main/docs/GETTING-STARTED.md).

## Sources

| | Source | Kind | Language | Type | Notes |
|---|---|---|---|---|---|
| <img src="sources/mangadex/icon.png" width="24"> | MangaDex | manga | en | plug-in | popular (new and followed), latest, search, details, every English chapter; no filters, 350 ms between requests |
| <img src="sources/weebcentral/icon.png" width="24"> | Weeb Central | manga | en | plug-in | filters, 2 s between requests |
| <img src="sources/kickassanime/icon.png" width="24"> | KickAssAnime | anime | en | plug-in | HLS with subtitles and audio tracks; details with synopsis, genres and status |
| <img src="sources/animepahe/icon.png" width="24"> | AnimePahe | anime | en | plug-in | the airing list, search, details, episodes and HLS in 360p to 1080p; needs an app that passes Cloudflare's check |
| <img src="sources/beatzanime/icon.png" width="24"> | BeatZ Anime | anime | es | recipe | browse, details and episodes; search and playback need engine work ([NEEDS-ENGINE.md](NEEDS-ENGINE.md)) |

Not offered (in `parked/`):

| Source | Why |
|---|---|
| Comix | the site retired the keys its API requests are signed with; it comes back when they are re-captured (`parked/comix/capture.js`) |
| KimoiTV | the site's Cloudflare Turnstile check blocks playback |

## How a source works

A source is one of two things:

- **A text recipe** (`recipe.toml`): CSS selectors and a few rules for simple sites. No code runs.
- **A WASM plug-in**: a small Rust crate built into a WebAssembly component. It runs in the app's sandbox, with no file
  or system access, and may reach only the hosts listed in its `manifest.toml`. Its interface (ABI 1.5) is described in
  the app's [docs/source-abi.md](https://github.com/anas1412/kurobayomi/blob/main/docs/source-abi.md).

Each source may also carry its site's icon, which the app shows in its source lists.

## Layout

```
sources/<id>/         one folder per source
  manifest.toml       a plug-in: id, name, lang, kind, version, hosts
  Cargo.toml, src/    the plug-in crate
  recipe.toml         or a text recipe: id, name, lang, kind, base_urls, selectors
  icon.png            the site's icon (square PNG)
  fixtures/           saved pages and responses the tests use
  README.md           what the source does, its original and licence, what was changed
parked/<id>/          sources kept but not offered
template/             starter crate for a new plug-in
dist/                 the files the index links to: <id>.wasm, <id>.toml, <id>.png (committed)
tools/build.sh        builds every plug-in into dist/
tools/make-index.py   copies recipes and icons into dist/ and writes index.min.json
index.min.json        generated; never edit by hand
```

## Add a source

**A text recipe**

1. Create `sources/<id>/recipe.toml`. Its `id` must equal the folder name; it also carries `name`, `lang`, `kind`
   (`manga` or `anime`) and an optional `version`.
2. Add `fixtures/` with saved pages, a `README.md`, and `icon.png` if the site has an icon.
3. Run `python3 tools/make-index.py`.

**A WASM plug-in**

1. `cp -r template sources/<id>`, rename the package in `Cargo.toml`, and write the ops in `src/lib.rs` against
   `wit/kuro-source.wit`.
2. Create `sources/<id>/manifest.toml`:

   ```toml
   id = "example"            # must equal the folder name
   name = "Example"
   lang = "en"
   kind = "manga"            # or "anime"
   version = 1
   hosts = ["example.org"]   # the only hosts the plug-in may call
   ```

3. Add `fixtures/`, a `README.md` and `icon.png`.
4. Run `tools/build.sh`, then `python3 tools/make-index.py`.

A plug-in must stay under 1 MiB. Bump `version` whenever a source changes, so the app's daily check fetches it again.

## Build and index

Once: `rustup target add wasm32-unknown-unknown` (`wasm-component-ld` ships with Rust's wasip2 support).

- `tools/build.sh` builds each plug-in for `wasm32-unknown-unknown` and turns it into a component with
  `wasm-component-ld`. It is not built for `wasm32-wasip2`: the sandbox offers no WASI, so a plug-in that imports
  `wasi:*` could not start, and both tools reject one.
- `tools/make-index.py` copies recipes and icons into `dist/`, checks the plug-ins, removes files in `dist/` that no
  entry names, and writes `index.min.json`.

Commit `dist/` and `index.min.json` together: the checksums in the index must match the files on the branch.

## Index format

```json
{
  "version": 1,
  "sources": [
    {
      "id": "example", "name": "Example", "lang": "en", "kind": "manga", "version": 1,
      "type": "wasm",
      "file": "https://raw.githubusercontent.com/<user>/<repo>/main/dist/example.wasm",
      "sha256": "<hex>",
      "hosts": ["example.org"],
      "icon": "https://raw.githubusercontent.com/<user>/<repo>/main/dist/example.png",
      "iconSha256": "<hex>"
    }
  ]
}
```

`hosts` is set for plug-ins only; `icon` and `iconSha256` are optional. All links must be https. A repository lists at
most 10 sources of each kind.

## Licence

Apache License 2.0. The sources are ports of Aniyomi and Mihon extensions; the site icons are those extensions' launcher
icons. See [NOTICE](NOTICE).
