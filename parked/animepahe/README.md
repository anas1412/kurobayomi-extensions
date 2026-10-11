# AnimePahe: BLOCKED

Status: **BLOCKED**. This source is not offered in the index and has no plug-in file. The site sits behind a Cloudflare
challenge that the plug-in host cannot pass. Why, and what would unblock it, is in [BLOCKED.md](BLOCKED.md).

This folder holds only the pure logic that stays valid once that feature exists. It has no network code and no site
fixtures. The tests use strings built from the reference code.

## What is here

| File | Ported from (reference, read only) | What it does |
|---|---|---|
| `src/unpacker.rs` | `lib/unpacker/.../JsUnpacker.kt`, `Unbaser.kt` | Dean Edwards P.A.C.K.E.R. unpacker: `unpack_and_combine` |
| `src/kwik.rs` | `extractor/KwikExtractor.kt` | Kwik parameters (`find_params`), the key routine (`decrypt`), the form fields (`form_fields`), the `/i` redirect (`kwik_url_from_location`), the stream URL (`hls_source`) |
| `src/dto.rs` | `dto/AnimePaheDto.kt` | serde shapes of the API pages and episode rows |

Reference (read only): the yuzono_anime-extensions tree, `src/en/animepahe/` and `lib/unpacker/`.

## Not ported (needs the host feature in BLOCKED.md)

- Every fetch: the airing, search, release and anime pages, and the Kwik page and its POST.
- The Cloudflare bypass (`CloudflareBypass.kt`, `CloudflareInterceptor.kt`) and the cookie handling.
- The HTML selectors of `AnimePahe.kt`, and the search, filter and episode URL building.
- `AnimePaheHlsServer.kt`.
- Latest updates: the reference sets `supportsLatest = false`, so a port answers `latest` as unsupported.

## Changes from the reference

- The 95-character unbaser alphabet is the printable ASCII set. The reference's Kotlin copy has one extra backslash (96
  entries), so for radix 63 to 95 this port decodes differently (for example `unbase("10", 95)` is 1631 here, 1727 in the
  reference). Radix up to 62 is identical. Whether Kwik uses a radix above 62 is unverified (kwik.cx was never reached).
- `hls_source` returns `None` when `const source=\'` is missing. The reference returns the whole text.
- `decrypt` returns `None` where the reference throws (a key index out of range). A piece that does not parse still stops
  the output, as in the reference.
- `\w` and `\d` are ASCII, as in Java.
- Dependencies are `regex` and `serde`, the crates the app's `Cargo.lock` already uses. `serde_json` is a dev dependency
  for the tests.

## Licence

This folder is part of this repository (Apache-2.0).

- `src/unpacker.rs` is ported from `lib/unpacker` of yuzono_anime-extensions, whose root LICENSE is Apache-2.0. That file
  names DatL4g/JsUnpacker as its origin; that licence was not checked.
- `src/kwik.rs` ports the Kwik routine, which comes from CloudStream's AnimePaheProvider under the MIT licence. The full
  notice (Copyright (c) 2021 Osten) is reproduced in the header of `src/kwik.rs`.

## Test coverage

The tests cover `decrypt`, `find_params` and `form_fields` only with `v2 = 8` and key indices 0 to 8. Key indices of 10 or
more and any radix other than 8 are not tested. With `v2` above 10 the decimal-digit scheme cannot invert an encoder for
digits 10 and up; the reference behaves the same, and this port matches it. Whether live Kwik pages use such values is unknown.

## Test

```
cd parked/animepahe && cargo test -j 1
```

## To unblock

1. The host gains a feature that returns the cookies and user agent after a challenge (see BLOCKED.md).
2. Add the fetch layer on top of `kwik` and `dto`, and a `manifest.toml` with `kind = "anime"` and the hosts allow-list
   (`animepahe.pw`, its redirect hosts, `kwik.cx` and the video CDN hosts).
3. Only then add the source to the index.
