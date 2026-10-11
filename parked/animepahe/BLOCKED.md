# AnimePahe: blocked, not ported

The live check failed, so the port stopped as the task requires. No code, manifest or `source.wasm` was written.

## What was seen (2026-10-11, 4 of 8 allowed requests, 1 s apart, text only)

| Request | Result |
|---|---|
| `GET https://animepahe.pw/api?m=airing&page=1` | HTTP 403, `content-type: text/html`, `cf-mitigated: challenge`, `server: cloudflare`, `<title>Just a moment...</title>` |
| `GET https://animepahe.com/api?m=airing&page=1` | HTTP 301 to `https://animepahe.pw/api?m=airing&page=1` |
| `GET https://animepahe.org/api?m=airing&page=1` | HTTP 301 to `https://animepahe.pw/api?m=airing&page=1` |
| `GET` of the redirected `.pw` URL (from `.com`) | the same 403 challenge |

The challenge body has `cType: 'precursor_interstitial'`, the text "Enable JavaScript and cookies to continue", and a Cloudflare challenge script with a per-response nonce. Every endpoint the reference uses (`/api?m=airing`, `/api?m=search`, `/api?m=release` for episodes, `/anime/{session}` for details, `/play/...` and the `/{session}/i` Kwik redirect) sits behind this challenge. Kwik (`kwik.cx`) was not reached.

## Why a sandboxed plug-in cannot pass it

- The reference gets past the challenge with an Android WebView (`extractor/CloudflareBypass.kt`) that runs the page's JavaScript, polls for the `cf_clearance` cookie, and returns that cookie with the user agent that solved it. `CloudflareInterceptor.kt` then retries the request with both.
- The ABI 1.4 host has four imports only: `http`, `pref-get`, `pref-set` and `log` (`crates/kuro-source-wasm/wit/kuro-source.wit` in the app repo). `http` is a plain HTTP client (`bridge.rs`). It forwards arbitrary request headers, so a `Cookie` and `User-Agent` could be sent, but no guest code can obtain a `cf_clearance` cookie.
- Host features that would unblock this (any one of them):
  1. A host WebView or browser import that loads a URL, runs its JavaScript and returns cookies plus the user agent that solved the challenge. This matches `CloudflareBypass.getCookies`, and its domain must be on the manifest allow-list.
  2. A JavaScript engine import that runs the challenge script. This is brittle, since it has to keep up with Cloudflare's managed challenges.
  3. A per-source cookie store that the user fills from their own browser session. This is a manual step with the user's own session cookie, and nothing in this lane implements it.
- Whichever route is chosen, the manifest `hosts` allow-list must also include `animepahe.pw` (and the redirect hosts) and `kwik.cx`, plus the video CDN hosts used for playback.

## What is portable once unblocked (not verified)

- The Kwik decode is pure string work and ports to Rust: the `eval(function(` packer unpack (`JsUnpacker`) and the `("key",n,"key2",v1,v2,n)` routine in `extractor/KwikExtractor.kt` (`decrypt`). The Kwik fetch also falls back to the same WebView bypass on 403 or 419, so it still needs the feature above.
- The JSON API shapes (`ResponseDto`, `LatestAnimeDto`, `SearchResultDto`, `EpisodeDto`) are in `dto/AnimePaheDto.kt` and are plain serde structs.
- The reference sets `supportsLatest = false`, so a port would answer `latest` as unsupported.

Reference source (read only): the yuzono_anime-extensions tree, `src/en/animepahe/`.

## Status update

The pure logic is now in this folder as a tested library crate, `parked/animepahe/` (`src/unpacker.rs`, `src/kwik.rs`,
`src/dto.rs`). See `README.md` there. The source stays blocked: no plug-in file, no manifest and no index entry.
