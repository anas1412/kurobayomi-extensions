# Comix (manga plug-in)

Comix (`https://comix.to`, mirror `comix.ws`) as a WASM plug-in for Kurobayomi, ABI 1.5 (package `kuro:source@1.5.0`,
`wit/kuro-source.wit`). Manifest: `manifest.toml` (version 2).

**Licence and origin.** The logic is ported from the keiyoushi `src/en/comix` extension (Apache-2.0), through the Rust
crate this replaces (`rust-sources/comix`, kept in place until the lead removes it). This port is Apache-2.0 as well. The
copy here is not the original file; it is the same logic in Rust for the plug-in contract.

## Ops

`info`, `create-sources`, `popular`, `latest`, `search`, `details`, `chapters`, `pages`, `request-headers` (ABI 1.5) and
`unscramble` (an addition the app must call for page images, see `../../docs/comix-image-hook.md`). Every other op is
"unknown op".

## ABI 1.5 (version 2)

- **Spacing.** `info` carries `requestGapMs: 200`, the host's gap between two requests to the site. The Kotlin is
  `rateLimit(5)`, five a second; a 200 ms gap is the same rate without the burst of five.
- **Page-image headers.** `request-headers` answers `[["Accept", "*/*"]]`: no Origin, no Referer, as the Kotlin
  `imageRequest`. The app uses this list for page images of the source.
- **Filters.** `search` reads the app's filter list by the Kotlin filter names (`Filters.kt`): Sort By (with its
  direction), Content rating, Type, Genres and Formats (include and exclude), Match, Demographic, Status, Minimum Chapter
  Length, From and To (release year), Tags, Author and Artist. The Tags, Author and Artist text is turned into site ids
  with a request to `/api/v1/tags/search` (not signed), as the Kotlin does. An absent filter takes its Kotlin default.
  The app holds no filters today, so a search sends the defaults.
- **Upload dates.** A chapter's `dateUpload` is read from the site's relative text ("5d ago") against the host clock
  (`unix-seconds`), in epoch milliseconds, as the Kotlin `parseRelativeDate`. Months and years move the calendar in UTC.
- **Retries.** A 404, 502, 503, 522 or 523 answer is asked again with `r=<attempt>` in the query, up to ten times, as the
  Kotlin `proceedWithRetry`. The Kotlin sleeps 1.5 s between tries; the guest cannot sleep, so the host's gap spaces them.
- **Details URL.** A title's URL is the one the detail gives (`url` after `/title`), as the Kotlin `toSManga`. Before, the
  address the page was read from was stored.

## What changed from the Rust crate

- **Transport.** The `http` import replaces the reqwest client, and the host allow-list is `manifest.toml`'s `hosts`.
  The rate limit is the host's job now: `requestGapMs` (see above). Before ABI 1.5 nothing spaced the requests.
- **Signature and cipher** are unchanged: the same keys (`src/keys/tmonyg.json`), the same SHA-256 resume code (the
  `sha2` crate cannot start from a state), the same byte-table cipher.
- **Descrambling** is unchanged (`src/descramble.rs`). It is also exposed as the `unscramble` op.
- **Details** read `<script id="initial-data">` by a scan, not with `scraper` (the script is raw text, so it ends at the
  first `</script>`).
- **URLs** are built as strings, not with the `url` crate; the encoding matches OkHttp's.
- **Source id** is unchanged: the MD5 of `comix/en/1`, the id the built-in source had, so library entries still match.
- **Left out:** the image fallbacks of the Kotlin client (the alt hosts in `#alt=` and the scramble path variants
  `/hi/`, `/fcf/`, ...): they retry the page image itself, which the app fetches, so the app owns them (see
  `../../docs/comix-image-hook.md`). Also left out: the scanlator blacklist and the deduplication (both off by default
  in the Kotlin), the Kotlin tag-id cache, related titles, deep links, the WebView fallbacks (the native cipher path is the
  one used), and the `live_check` example (it needs the native app crates).

## Build

```
cd sources/comix
cargo test                                                # pure logic, on the host
cargo build --release --target wasm32-unknown-unknown     # the plug-in
cp target/wasm32-unknown-unknown/release/kurobayomi_source_comix.wasm ../../dist/comix.wasm
```

The target is `wasm32-unknown-unknown` linked by `wasm-component-ld --wasi-adapter=none` (see `.cargo/config.toml`), so the
output is a component that imports only `kuro:source/host`. A `wasm32-wasip2` build is not usable here: it imports
`wasi:cli`, `wasi:clocks` and `wasi:random`, and the app's linker provides none of them. Check a build by decoding it (the
world must import `kuro:source@1.5.0::host` and export `invoke`, and nothing from `wasi:`); `wasm-tools` is not installed
in the sandbox, so this was checked with a scratch decoder built on `wit-component` 0.227.

The build needs `wit/kuro-source.wit` (a copy of the app's contract, `ABI 1.5`), `wit-bindgen` 0.41 and the
`wasm-component-ld` tool.

## Tests

The unit tests cover the signature tokens and the cipher checked against values the site produced, the parsers over
responses saved from the live site on 2026-10-10 (`fixtures/`), the descrambler against a model of the Kotlin integer
arithmetic, the op dispatch, the search filters (the Kotlin query each filter makes, and the defaults), the upload dates
against a fixed clock (`api::upload_date`), and the `info` and `request-headers` answers. They were written for ABI 1.5 and
have not been run in this change (the lead builds). Nothing runs against the live site from the sandbox.

## Keys and a new site build

The keys belong to site build `tmonyg`. `src/keys/tmonyg.json` was read from the site's public client code, is specific to that build and goes stale on the next one; it is not a credential. When the site rebuilds, requests fail with "Comix refused the request signature".
Run `capture.js` in a page of `comix.to` (it returns the JSON `Keys::from_json` reads), replace `src/keys/tmonyg.json`,
and rebuild.
