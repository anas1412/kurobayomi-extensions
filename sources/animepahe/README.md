# AnimePahe

AnimePahe (<https://animepahe.pw>) as an anime source plug-in for Kurobayomi (ABI 1.5, `wit/kuro-source.wit`, anime ops).

The site answers plain clients with Cloudflare's browser check. The app handles that, not this plug-in: when a request
gets the check, the app opens the site once in a hidden WebKit page, which passes it, and sends the site's requests from
that page from then on (`src/page_fetch.rs` in the app). The video CDN refuses HTTP/1.1, so the app's player plays the
HLS streams through its local relay (`src/player_view/hls_proxy.rs` in the app). An app older than that cannot use this
source.

## Ops

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor, id `9163675745699695675` (`MD5("animepahe/en/1")`, the Kotlin id); no Latest; 1.5 s between requests |
| `popular` | `GET /api?m=airing&page=N` (the site has no popular list; the Kotlin shows this one). The list carries a screenshot, not a poster: a remembered poster replaces it, else the thumbnail is `kb-cover:/a/{id}` and the app asks `cover` when the card shows |
| `cover` | the poster of one anime: the remembered one, else found by searching its title (kept by `popular`) and matching the id |
| `search` | `GET /api?m=search&q=<text> <stamp>&page=N` (the stamp gets past the site's cache; the search ignores it). No text: the airing list |
| `details` | `GET /anime/{session}`: title, poster, summary, genres, status, studio |
| `episodes` | every page of `GET /api?m=release&id={session}&sort=episode_asc&page=N`, newest first |
| `hosters` | `GET /play/{session}/{episode}`: one hoster per fansub and language ("Sub" first), its streams best first |
| `videos` | each stream's Kwik page, its packed script unpacked to the HLS playlist; Referer the Kwik page |

An anime is kept as `/a/{id}`: the id never changes, while the site's session ids change every few days. Sessions are
cached in the plug-in's preferences from every list it reads; when one no longer works (404) the plug-in searches the
title, matches the id and retries once.

## Licence

This folder is part of this repository (Apache-2.0). Ported from the yuzono `en/animepahe` extension for Aniyomi.

- `src/unpacker.rs` is ported from `lib/unpacker` of yuzono_anime-extensions, whose root LICENSE is Apache-2.0. That file
  names DatL4g/JsUnpacker as its origin; that licence was not checked.
- `src/kwik.rs` ports the Kwik routine, which comes from CloudStream's AnimePaheProvider under the MIT licence. The full
  notice (Copyright (c) 2021 Osten) is reproduced in the header of `src/kwik.rs`.

## Not ported

- The MP4 (pahe.win) links: the decode is in `src/kwik.rs` for downloads later; streaming uses HLS.
- The genre, theme, demographic and season filters.
- The settings (quality, language, AV1, domain, user agent): fixed at the Kotlin defaults.

## Test

```
cd sources/animepahe && cargo test
```

The fixtures were saved from the live site on 2026-10-14.
