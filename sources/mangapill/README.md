# MangaPill

MangaPill (<https://mangapill.com>) as a manga source plug-in for Kurobayomi (ABI 1.5, `wit/kuro-source.wit`, manga ops).

## Licence and origin

Licensed under the Apache License 2.0. Ported from the keiyoushi `en/mangapill` extension (Apache-2.0). The selectors
follow it. The source id is the Kotlin one: `MD5("mangapill/en/1")` taken as its first 8 bytes, big-endian, with the top bit
cleared: `8448310129093543312`.

## Ops

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; Latest; 1 s between requests |
| `request-headers` | the page-image headers: the site as Referer |
| `popular` | `GET /`: the "Trending Mangas" cards (the site has no popular list; one page) |
| `latest` | `GET /chapters`: the latest chapters as manga cards (one page) |
| `search` | `GET /search?page=N&q=...`; there is a next page when the page has a "Next" link |
| `details` | `GET /manga/{id}/{slug}`: title, summary, genres, status, cover |
| `chapters` | the same page: `#chapters` links, newest first, with the number read from the name. The site gives no dates |
| `pages` | `GET /chapters/{id}/{slug}`: the `picture img` addresses in order |

## Not ported

- The search filters (genre, status, type).
- The author and artist: the site gives none (the Kotlin sets them empty).

## Test

```
cd sources/mangapill && cargo test
```

The fixtures were saved from the live site on 2026-10-14 and trimmed to a few cards and chapters.
