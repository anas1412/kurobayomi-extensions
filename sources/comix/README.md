# Comix (manga plug-in)

Comix (`https://comix.to`, mirror `comix.ws`) as a WASM plug-in for Kurobayomi, ABI 1.4. Manifest: `manifest.toml`.

**Licence and origin.** The logic is ported from the keiyoushi `src/en/comix` extension (Apache-2.0), through the Rust
crate this replaces (`rust-sources/comix`, kept in place until the lead removes it). This port is Apache-2.0 as well. The
copy here is not the original file; it is the same logic in Rust for the plug-in contract.

## Ops

`info`, `create-sources`, `popular`, `latest`, `search`, `details`, `chapters`, `pages`, and `unscramble` (an addition the
app must call for page images, see `../../docs/comix-image-hook.md`). Every other op is "unknown op".

## What changed from the Rust crate

- **Transport.** The `http` import replaces the reqwest client, and the host allow-list is `manifest.toml`'s `hosts`.
  The 5-requests-per-second rate limit of the original is not kept by the plug-in; the host must enforce it, and the app does not yet: until it sets a per-host limit (5 per second for comix.to), Comix is unthrottled in the app, and `chapters` can send up to 200 signed requests back to back.
- **Signature and cipher** are unchanged: the same keys (`src/keys/tmonyg.json`), the same SHA-256 resume code (the
  `sha2` crate cannot start from a state), the same byte-table cipher.
- **Descrambling** is unchanged (`src/descramble.rs`). It is also exposed as the `unscramble` op.
- **Details** read `<script id="initial-data">` by a scan, not with `scraper` (the script is raw text, so it ends at the
  first `</script>`).
- **URLs** are built as strings, not with the `url` crate; the encoding matches OkHttp's.
- **Source id** is unchanged: the MD5 of `comix/en/1`, the id the built-in source had, so library entries still match.
- **Left out:** filters (ABI 1.4 has no filter op, so search uses the Kotlin defaults), the chapter upload date (the plug-in
  has no clock, so `dateUpload` is 0), the Kotlin tag, author and artist filters, related titles, deep links, the scanlator
  blacklist, the WebView fallbacks, and the `live_check` example (it needs the native app crates).

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
world must import `kuro:source@1.4.0::host` and export `invoke`, and nothing from `wasi:`); `wasm-tools` is not installed
in the sandbox, so this was checked with a scratch decoder built on `wit-component` 0.227.

The build needs `wit/kuro-source.wit` (a copy of the app's contract, `ABI 1.4`), `wit-bindgen` 0.41 and the
`wasm-component-ld` tool.

## Tests

26 unit tests: the signature tokens and the cipher checked against values the site produced, the parsers over responses saved
from the live site on 2026-10-10 (`fixtures/`), the descrambler against a model of the Kotlin integer arithmetic, and the
op dispatch. Nothing runs against the live site from the sandbox; a live check needs the app.

## Keys and a new site build

The keys belong to site build `tmonyg`. `src/keys/tmonyg.json` was read from the site's public client code, is specific to that build and goes stale on the next one; it is not a credential. When the site rebuilds, requests fail with "Comix refused the request signature".
Run `capture.js` in a page of `comix.to` (it returns the JSON `Keys::from_json` reads), replace `src/keys/tmonyg.json`,
and rebuild.
