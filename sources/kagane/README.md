# Kagane

Kagane (<https://kagane.si>) as a manga source plug-in for Kurobayomi (ABI 1.5, `wit/kuro-source.wit`, manga ops), English only.

## Licence and origin

Licensed under the Apache License 2.0. Ported from the keiyoushi `all/kagane` extension (Apache-2.0). The API paths, the
title, chapter and page rules follow it. The source id is the Kotlin one: `MD5("kagane/en/1")` taken as its first 8
bytes, big-endian, with the top bit cleared: `4024736764982024684`.

## Ops

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; Latest; 350 ms between requests (the Kotlin allows 3 a second) |
| `request-headers` | the page-image headers: `Accept` (JPEG XL first) and `Referer` |
| `popular` | `POST /api/v2/search/series?sort=total_views,desc`, English, every source type, 35 a page |
| `latest` | the same with `sort=updated_at,desc` |
| `search` | the same with the title and no sort (relevance). No filters |
| `details` | `GET /api/v2/series/{id}`: rating, summary, links and other titles in the description; format and genres; authors and artists from the staff roles; status; the first cover |
| `chapters` | `GET /api/v2/series/{id}/books/index?fields=List`, the compact index, highest sort number first; the groups are the scanlator |
| `pages` | `POST /api/integrity` for the integrity token (kept until it runs out), then `POST /api/v2/books/{book}` with it: the cache host, the access token and the pages, as `{cache}/api/v2/books/page/{book}/{page}.{ext}?token=...` |

Content shown: every content rating, from every source type (Official, Unofficial and Mixed), as the Kotlin defaults.
Covers and page images load in the app, so the manifest lists only `kagane.si`.

## Not ported

- The other 17 languages, the filters (genres, tags, sources, content rating, sort), the settings (content rating,
  excluded genres, official sources only, title cleaning, edition and source names in titles, chapter title mode, data
  saver), related titles, and the group tag the Kotlin reads out of a chapter's title.
- The page token refresh: an access token runs out after a while, so a chapter left open a long time may need to be
  opened again.
