# kuro-src-comix

Comix (`https://comix.to`, mirror `comix.ws`) as a built-in manga source, ported from the keiyoushi
`src/en/comix` extension (Apache-2.0). Register it with `kuro_src_comix::source(client)`; it implements
`MangaSource` and `CatalogueSource` (popular, latest, search with filters, details, chapters, pages).

## How the site works now (checked 2026-10-10, site build `tmonyg`)

The Kotlin source caught three S-boxes and keys from `atob` in a WebView and signed with a substitution
cipher (`Cipher.kt`). The site no longer does that. Watching its own `secure-tmonyg-*.js` in a browser shows:

- **Signature.** Every `/api/v1/...` call needs `_ = "17w." + base64url(HMAC-SHA256(key, text)[..12])`, where
  `text` is the path without `/api/v1` plus `?` and the canonical query (Kotlin `canonicalEntries`: names sorted,
  lists numbered `name[0]`, raw values). The page only holds the two SHA-256 states after the key block, so
  `src/keys/tmonyg.json` keeps those (`inner`, `outer`). Without `_` the API answers 403 `missing_token`.
- **Cipher.** Chapter lists and page lists come back as `{"e": base64}` (header `x-enc: 2`). Byte 0 and 1 pick a
  start in a fixed 1024-byte table; the rest is XOR with the table from there, wrapping. Lists of mangas and the
  title HTML are plain.
- **Details** come from the title page's `script#initial-data` (no signature needed), as in Kotlin.

The tests check the signature against nine tokens the site's own code produced, the decryption against a real
encrypted response and its plain text, and the parsers against saved real responses (`tests/fixtures`).

## What the lead must wire

1. **Registration**: `kuro_src_comix::source(client)` returns `Comix`.
2. **Page images** (the reader cannot reach any of this through `MangaSource`):
   - Request `page.image_url` with `Comix::image_headers()`: no `Referer`, no `Origin` (the image hosts refuse them).
   - Pass the response body and headers to `kuro_src_comix::descramble::unscramble(&body, &headers)`.
     `Ok(None)` = use the body as is; `Ok(Some(bytes))` = show these bytes (XOR-decoded, or the 5x5 grid put back
     and re-encoded as JPEG 90); `Err` = the grid image did not decode (Kotlin answers 500).
   - Retries (Kotlin `altHostFallback` and `proceedWithRetry`): the URL fragment `#alt=h1,h2,...` lists hosts to
     retry on, in order, with query `r=1`, `r=2`, ... and 2 s between tries; on 502/503/522/523/404 retry the
     same URL with `r=<attempt>` up to 10 times, 1.5 s apart; after that try the path variants `/hi/`, `/fcf/`,
     `/i5/`, `/si/`, `/i/`, `/sii/`, `/ii/` in place of the first `/(i5|s?i+)/`. Strip the fragment before sending.
3. **New site build = new keys.** The keys belong to build `tmonyg`. When the site rebuilds, calls fail with
   "Comix refused the request signature ... recapture them with capture.js" (and a `x-build-id` mismatch is logged
   as a warning first). The hook needed: a WebView call that loads `https://comix.to/`, waits for the page, evaluates
   `capture.js` (crate root; it returns a Promise of a JSON string) and passes the string to
   `Comix::set_keys(Keys::from_json(&json)?)`. `capture.js` was run in a browser on a fresh page load and gave
   exactly the bundled keys. `WebViewHost` has no "evaluate a script and return its result" method yet; that method
   is what is missing.

## Left out (Kotlin has them)

- Preferences (poster quality, content rating, default types and demographics, blocked genres, deduplication,
  scanlator blacklist, description options): the Kotlin defaults are built in.
- Tags, author and artist text filters (Kotlin resolves names to ids with extra requests).
- Related mangas, deep links (`getMangaByUrl`), "fetch chapters until known".
- The WebView fallbacks: the native signed API covers every call.

## Live check

`cargo run -p kuro-src-comix --example live_check` runs popular, latest, search, details, chapters and pages
once, one request per second, and exits non-zero when a step fails or returns nothing.
