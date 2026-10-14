# Mangakakalot

Mangakakalot (<https://www.mangakakalot.gg>) as a manga source plug-in for Kurobayomi (ABI 1.5, `wit/kuro-source.wit`, manga ops).

The site answers plain clients with Cloudflare's browser check. The app handles that, not this plug-in: when a request gets
the check, the app opens the site once in a hidden web page, which passes it, and sends the site's requests from that page
from then on (`kuro_platform::hidden_pages` in the app). An app without it cannot use this source.

## Licence and origin

Licensed under the Apache License 2.0. Ported from the keiyoushi `en/mangakakalot` extension and its MangaBox base
(Apache-2.0). The selectors and the chapter API follow it. The source id is the Kotlin one: `MD5("mangakakalot/en/1")`
taken as its first 8 bytes, big-endian, with the top bit cleared: `2528986671771677900`.

## Ops

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; Latest; 500 ms between requests (the Kotlin allows 2 a second) |
| `request-headers` | the image headers: the site as Referer (the CDN answers 403 without it, covers included) |
| `popular` | `GET /manga-list/hot-manga?page=N` |
| `latest` | `GET /manga-list/latest-manga?page=N` |
| `search` | `GET /search/story/{query as a slug}?page=N`; no text: the hot list |
| `details` | `GET /manga/{slug}`: title, author, status, genres, summary with the alternative name, cover |
| `chapters` | `GET /api/manga/{slug}/chapters?limit=-1`: every chapter with its date, newest first |
| `pages` | `GET /manga/{slug}/{chapter}`: the image paths of its script on the first CDN, else the reader's images |

A listing has a next page when its pager has a link after the selected page that is not "Last" (the Kotlin treats any pager
as more).

## Not ported

- The genre, status and sort filters.
- The other CDNs: the Kotlin retries a failing image on the chapter's other CDNs, the app cannot yet.
- The option that merges strip images into taller ones.
- The second domain (`mangakakalove.com`).

## Test

```
cd sources/mangakakalot && cargo test
```

The fixtures were saved from the live site on 2026-10-14 and trimmed to a few cards and chapters.
