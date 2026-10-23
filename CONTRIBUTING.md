# Contributing

Fixes for broken sources and new sources are welcome. This page covers how a source is built, how to try it in the app,
and what a pull request needs.

## Before you start

- **Bug reports:** open an issue with the source's name, what you did and what happened.
- **New sources:** open an issue first, so two people do not port the same site.
- **Ports only:** every source here is a port of an open-source Aniyomi, Mihon or keiyoushi extension under
  Apache-2.0. Name the original in the source's README. Do not copy code under another licence.

## Two kinds of source

| | Text recipe | WASM plug-in |
|---|---|---|
| File | `recipe.toml` | a Rust crate built to `.wasm` |
| Good for | simple HTML sites: lists, pages, links | APIs, signed requests, JavaScript unpacking, HLS |
| Runs code | no, only CSS selectors and a few rules | yes, in the app's sandbox, with no file or system access |
| Reference | [source-recipes.md](https://github.com/anas1412/kurobayomi/blob/main/docs/source-recipes.md) | [source-abi.md](https://github.com/anas1412/kurobayomi/blob/main/docs/source-abi.md) |

Try a recipe first. Use a plug-in when the site needs real logic.

## Repository layout

```
sources/<id>/         one folder per source; <id> is lower-case letters, digits, '.', '_', '-'
  recipe.toml         a recipe: id, name, lang, kind, version, base_urls, selectors
  manifest.toml       or a plug-in: id, name, lang, kind, version, hosts
  Cargo.toml, src/    the plug-in crate (standalone, not in a workspace)
  fixtures/           saved pages and responses the tests read
  icon.png            the site's icon, square PNG
  health.toml         the page the health check fetches to see the site is up (url, expect, cloudflare)
  README.md           what it does, what was left out, the original extension
parked/<id>/          sources kept but not offered; never read by the tools
template/             starter crate for a new plug-in
status.toml           sources marked blocked or broken by hand (overrides health.toml)
health.toml           written by tools/check-health.py: sources found broken (do not edit)
health-state.json     the failures in a row per source, for the next check
tools/check-health.py checks every source's site; a scheduled workflow runs it and rebuilds the index
tools/build.sh        builds every plug-in into dist/
tools/make-index.py   copies recipes and icons into dist/ and writes index.min.json
dist/, index.min.json generated, but committed: the app downloads from them
```

## Add a recipe

1. Create `sources/<id>/recipe.toml`. Its `id` must be the folder name. It also needs `name`, `lang`, `kind`
   (`manga` or `anime`) and `version` (default 1). The keys are in
   [source-recipes.md](https://github.com/anas1412/kurobayomi/blob/main/docs/source-recipes.md).
2. Save a few real pages from the site into `fixtures/` and point the recipe's `[tests]` section at them.
3. Check it from a clone of the [app repository](https://github.com/anas1412/kurobayomi):

   ```bash
   cargo run -p kuro-source-scrape --example check_recipe -- /path/to/recipe.toml /path/to/fixtures/
   ```

   Add `--live` to run it against the real site too.
4. Add `icon.png` and a `README.md`.

## Add a plug-in

One-time setup:

```bash
rustup target add wasm32-unknown-unknown
```

`wasm-component-ld` ships with Rust's wasip2 support, so there is nothing else to install.

1. Copy the template and rename the package in `Cargo.toml`:

   ```bash
   cp -r template sources/<id>
   ```

2. Write the operations in `src/lib.rs` against `wit/kuro-source.wit`. The existing plug-ins (`sources/mangadex` for an
   API, `sources/mangapill` for HTML, `sources/animepahe` for HLS) are good models.
3. Create `sources/<id>/manifest.toml`:

   ```toml
   id = "example"            # must equal the folder name
   name = "Example"
   lang = "en"
   kind = "manga"            # or "anime"
   version = 1
   hosts = ["example.org"]   # the only hosts the plug-in may call
   ```

   List only the hosts the plug-in requests itself. Images and videos load in the app and do not need to be listed.
4. Save responses into `fixtures/` and test the parsing against them:

   ```bash
   cd sources/<id> && cargo test
   ```

5. Add `icon.png` and a `README.md`.

A built plug-in must be under 1 MiB. `tools/build.sh` fails when it is bigger, and also when it imports anything but the
app's host interface. Build for `wasm32-unknown-unknown` through the script, never for `wasm32-wasip2`: the sandbox has
no WASI, so a wasip2 build cannot start.

## Fix a source

Make the change, then bump `version` in its `manifest.toml` or `recipe.toml`. Without a new version the app keeps
the copy it already has.

If a source stops working and you cannot fix it, mark it in `status.toml` so users see why:

```toml
example = "blocked"   # the site refuses the app for now (orange dot)
example = "broken"    # the source needs a fix (red dot)
```

Remove the line once it works again.

A scheduled workflow (`.github/workflows/health.yml`, every 6 hours) also checks each site by itself with
`tools/check-health.py` and marks a source broken after two failed checks in a row, active again at the next good one.
Cloudflare's browser check counts as "unknown", not as down: the app passes it, this runner cannot. Give a new source a
`health.toml` (`url`, `expect`, `cloudflare = true` when the site has that check, and `user_agent` when the site
answers only the User-Agent the source sends). The rules are tested with
`python3 tools/test-check-health.py`.

## Build the index

After any change to a source, run both from the repository root:

```bash
tools/build.sh
```

```bash
python3 tools/make-index.py
```

`build.sh` builds each plug-in into `dist/<id>.wasm`. `make-index.py` copies recipes and icons into `dist/`, checks every
source, removes files no source names, and writes `index.min.json` with each file's checksum.

Commit `dist/` and `index.min.json` together with the source. The checksums in the index must match the files on the
branch, or the app refuses the download. Never edit `index.min.json` by hand.

## Try it in the app

Point an index at your fork and add that link in Kurobayomi:

```bash
python3 tools/make-index.py --base-url https://raw.githubusercontent.com/<you>/kurobayomi-extensions/<branch>
```

Push the branch, then add
`https://raw.githubusercontent.com/<you>/kurobayomi-extensions/<branch>/index.min.json` under **Browse → menu →
Repositories → Add**. Run `make-index.py` again without `--base-url` before you open the pull request, so the index
points at this repository.

A recipe can also be tried without a repository: copy `recipe.toml` to `~/.local/share/kurobayomi/scrapers/<id>.toml`
(`kurobayomi-nightly` for the nightly app) and restart the app.

## Pull request checklist

- [ ] The source's README names the extension it was ported from and what was left out.
- [ ] `version` is bumped (for a fix) or set to 1 (for a new source).
- [ ] Tests pass against the saved fixtures.
- [ ] `tools/build.sh` and `python3 tools/make-index.py` were run, and `dist/` and `index.min.json` are committed.
- [ ] The index points at this repository, not your fork.
- [ ] For a new source, the README's source table has a row for it.

By contributing you agree that your work is licensed under the Apache License 2.0, like the rest of this repository.
