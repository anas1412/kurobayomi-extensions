# MangaDex

MangaDex (<https://mangadex.org>) as a manga source plug-in for Kurobayomi (ABI 1.5, `wit/kuro-source.wit`, manga ops), English only.

## Licence and origin

Licensed under the Apache License 2.0. Ported from the keiyoushi `all/mangadex` extension (Apache-2.0). The API paths,
the title, chapter and genre rules follow it. The source id is the Kotlin one: `MD5("mangadex/en/1")` taken as its first
8 bytes, big-endian, with the top bit cleared: `2499283573021220255`.

## Ops

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; Latest; 350 ms between requests (the Kotlin allows 3 a second) |
| `request-headers` | the page-image headers: `Accept` only |
| `popular` | `GET /manga?order[followedCount]=desc`, English chapters, created in the last 30 days (as the Kotlin), 20 a page |
| `latest` | `GET /chapter` (newest English chapters, 100 a page), then `GET /manga?ids[]=...` for their titles and covers, in the chapters' order |
| `search` | `GET /manga?title=...`; no text lists the most followed titles. No filters |
| `details` | `GET /manga/{id}` with cover, authors and artists: summary, genres (tags, demographic, content rating), status |
| `chapters` | every page of `GET /manga/{id}/feed` (English), newest first. Chapters that only link to another site are left out, as the Kotlin does |
| `pages` | `GET /at-home/server/{chapter}`: the image server, then `/data/{hash}/{file}` for each page |

Content shown: safe and suggestive, from every original language (the Kotlin defaults). Covers and page images load in the
app, so the manifest lists only `api.mangadex.org`.

## Not ported

- The filters, the `id:`, `ch:`, `grp:`, `author:`, `usr:` and `list:` search prefixes, and the settings (content ratings,
  languages, data saver, blocked groups, port 443, cover quality).
- The first-volume cover: the cover is the one the manga lists (a later volume's may differ).
- The page-image token refresh: image addresses from `pages` work for about 15 minutes after the call.

## Test

```
cd sources/mangadex && cargo test
```

The fixtures were saved from the live API on 2026-10-14.
