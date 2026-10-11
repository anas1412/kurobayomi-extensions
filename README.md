# Kurobayomi extensions

Sources for [Kurobayomi](https://github.com/anas1412/kurobayomi): one folder per source, manga in `manga/`, anime in `anime/`.
Each source is a small text recipe (`recipe.toml`) plus saved page samples (`fixtures/`) that the tests use. Folders whose
name starts with `_` are parked and are not offered.

Sources are ported from open-source Aniyomi and Mihon extension repositories (Apache-2.0). Each source's README names the
original and what was changed.

## How to add this repository to the app

Kurobayomi ships with no online sources. You add a repository by its link, and its sources appear at once. There is no
install step.

1. In Kurobayomi open **Browse**, then the menu, then **Extension repos**, and add this link:
   `https://raw.githubusercontent.com/anas1412/kurobayomi-extensions/main/index.min.json`
2. Browse lists the sources of the repository, grouped by language, with a language filter.
3. A source's file is downloaded the first time you open that source, and its checksum is checked against the index.
   The daily update check refreshes the index and the files.
4. To remove the repository, remove its link in **Extension repos**; its sources go with it.

A repository lists at most 10 sources of each kind (manga and anime).

Developers can test a recipe without a repository: copy it to `~/.local/share/kurobayomi/scrapers/<id>.toml` and restart the
app.

## The index

`index.min.json` is generated from the folders, so never edit it by hand:

```
python3 tools/make-index.py
```

Each entry has `id`, `name`, `lang`, `kind` (`manga` or `anime`), `version`, `file` (the raw https link of the source's
`recipe.toml`) and `sha256` (of that file). Commit the index after you change a recipe, because the checksums must match
the files on the branch.

## Plug-in sources (WASM)

`wasm-template/` is a guest crate skeleton for a source written as a WebAssembly plug-in. It implements the contract in
`wasm-template/wit/kuro-source.wit` (ABI 1.4): the `info` and `create-sources` ops are written, and the rest answer the
contract's "unknown op". The anime ops (`episodes`, `hosters`, `videos`, `subtitles`) are described in the WIT file.

Build a plug-in:

```
cd wasm-template
cargo build --release --target wasm32-unknown-unknown
wasm-tools component new target/wasm32-unknown-unknown/release/kurobayomi_source_template.wasm -o source.wasm
```

The index lists recipes only for now; a plug-in file is not listed yet.

## Sources

| Source | Kind | Status |
|---|---|---|
| `manga/existentialcomics` | manga | works |
| `manga/gwtb` (Gone with the Blastwave) | manga | recipe done, chapter links fixed in the engine |
| `anime/beatzanime` | anime | browse and episode list work, video playback is not working yet |
| `anime/_parked/kimoitv` | anime | parked: the site's bot check blocks playback |

More sources (WeebCentral, Comix, KickAssAnime) are in `rust-sources/` and will be added as plug-ins.
