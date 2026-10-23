# Senkuro

Senkuro (<https://senkuro.com>) as a manga source plug-in for Kurobayomi (ABI 1.5, `wit/kuro-source.wit`, manga ops),
Russian.

## Licence and origin

Licensed under the Apache License 2.0. Ported from the keiyoushi `ru/senkuro` extension and its `lib-multisrc/senkuro`
base (Apache-2.0). The GraphQL queries, the request headers, the title, details, chapter and page rules follow it. The
source id is the Kotlin one: `MD5("senkuro/ru/1")` taken as its first 8 bytes, big-endian, with the top bit cleared:
`1452500215874655811`. Manga and chapter urls keep the Kotlin's `{id},,{slug}` form, so Aniyomi backups match.

## Ops

Every op is one `POST /graphql` to the API.

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; no Latest (the Kotlin has none); 350 ms between requests (the Kotlin allows 3 a second) |
| `popular` | `searchTachiyomiManga` sorted by popularity, 10 a page |
| `search` | the same with the query. No filters |
| `details` | `fetchTachiyomiManga`: the Russian title, other titles and the Russian summary in the description; type, age rating, formats and labels as genres; authors and artists from the staff roles; status; cover |
| `chapters` | `fetchTachiyomiChapters`: every chapter, named `{volume}. Глава {number} {name}`, with its date and teams as the scanlator |
| `pages` | `fetchTachiyomiChapterPages`: the page images on `mirai.senkuro.net` (the first is the site's own credit page, as in the Kotlin) |

Adult titles (rated 18+, `EXPLICIT`) follow the app's **Show adult (18+) titles** switch (Settings, NSFW group, off by
default, sent to the plug-in as `showAdult`): while it is off, the list and search leave them out.

## Mirrors and blocking

The site has two addresses, as in the Kotlin: `senkuro.com` (public) and `senkuro.me` (for Russia). `senkuro.me` is
geo-blocked by DDoS-Guard outside Russia, and `senkuro.com` may be unreachable from inside it. The plug-in tries the
mirror that answered last, then the other one, and keeps whichever answers, so there is no setting to choose. Tested
from outside Russia: `senkuro.com` answers every op. Not tested from inside Russia. No login is needed.

## Not ported

- The search filters (type, format, status, translation status, age rating, and the label groups the Kotlin fetches
  from the API): the app's plug-ins declare no filters yet.
- The sort order choice (rating or popularity): the list is always by popularity.
- The mirror setting: replaced by the automatic fallback above.
