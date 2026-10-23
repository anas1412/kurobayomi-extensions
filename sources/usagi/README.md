# Usagi

Usagi (<https://web.usagi.one>), a Russian manga site of the GroupLe network (ReadManga, MintManga), as a manga source
plug-in for Kurobayomi (ABI 1.5, `wit/kuro-source.wit`, manga ops).

## Licence and origin

Licensed under the Apache License 2.0. Ported from the keiyoushi `ru/usagi` extension (Apache-2.0, version 1), which is
the keiyoushi GroupLe multisrc (`lib-multisrc/grouple`, base version 43) with site id 12 and no overrides. The selectors,
the catalog API and the page-list rules are the Kotlin's. The source id is the Kotlin one: `MD5("usagi/ru/1")` taken as
its first 8 bytes, big-endian, with the top bit cleared: `5705163226489175947`. Manga urls are the site's paths
(`/33450`), chapter urls carry the Kotlin's `?mtr=true`, so Aniyomi backups map onto it.

It is a plug-in, not a recipe: the page images are listed in a `rm_h.readerInit(...)` call in the chapter's inline
script, which a recipe cannot read.

## Ops

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; Latest; 500 ms between requests (the Kotlin's `rateLimit(2)`) |
| `request-headers` | the site as Referer and the User-Agent `arora` |
| `popular` | `GET /api/catalog/search?offset=N&sortType=RATING`, 50 titles a page |
| `latest` | the same with `sortType=DATE_UPDATE` |
| `search` | the same with `q=...`, sorted by rating |
| `details` | `GET /{title}`: title, authors, artists, category, age rating and tags as genres, status from the release and translation rows, rating, summary and alternative names in the description, cover |
| `chapters` | the same page: the chapter table, newest first, with number, date, translator and the Kotlin's name clean-up |
| `pages` | `GET /{title}/volN/M?mtr=true`: the image addresses from `rm_h.readerInit` (or `readerDoInit`), the token query dropped for `one-way.work` |

Every request sends the User-Agent `arora`, the GroupLe default. From here (2026-10-23) the site answers a browser
User-Agent with 404 on every page, and the Kotlin's setting says some countries need it. For the same reason
`health.toml` sets `user_agent`.

## Not ported

- **Sign-in.** The Kotlin's automatic sign-in is switched off for Usagi itself (the site sends signed-in users to
  ReadManga), so nothing is lost. Titles the site hides from guests (`blockedForAnonymous`) answer the chapter list with
  the Kotlin's "sign in" error, and licensed titles with its "removed by the right holder" error. A chapter page that
  answers 404 (as the chapters of such titles do) gives the Kotlin's "sign in" error too.
- **The user hash** (`?d=...`) the Kotlin adds for signed-in users.
- **The search filters** (sort, genres, tags, categories, statuses, years): the Kotlin fetches them from the site at run
  time.
- **The User-Agent setting** and **related titles** (a Komikku extra).
- **Paid chapters** answer the Kotlin's "this chapter is paid" error.

No login is needed for the rest, and nothing was geo-blocked in the checks above.

## Test

```
cd sources/usagi && cargo test
```

The fixtures were saved from the live site on 2026-10-23: the first six titles of the popular and latest catalogs, a
search for "берсерк", the title page of `/33450` and its chapter 41. A second title
(`/heavenly_demon_cultivation_simulation`: 197 chapters, 18 pages) was checked live the same day with the same parsers.
