# AnimeParadise

AnimeParadise (<https://www.animeparadise.moe>) as an anime source plug-in for Kurobayomi (ABI 1.5,
`wit/kuro-source.wit`, anime ops), English.

## Licence and origin

Licensed under the Apache License 2.0. Ported from the Kohi-den `en/animeparadise` extension (Apache-2.0, version 1).
The source id is the Kotlin one: `MD5("animeparadise/en/1")` taken as its first 8 bytes, big-endian, with the top bit
cleared: `3319373396687010408`. The anime url (`{"slug":...,"id":...}`) and the episode url (`/watch/{uid}?origin={id}`)
keep the Kotlin's form, so entries from an Aniyomi backup carry over.

The site changed since that extension was written: the routes it used (`/?sort=...`, the `__NEXT_DATA__` pages,
`/storage/{title}/{number}`) are gone. This plug-in follows the site's current API, read from its web player.

## Ops

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; Latest; 300 ms between requests |
| `request-headers` | `Referer` and `Origin` of the site, for the stream and the subtitle files |
| `popular` | `GET /search?page=N&sort=POPULARITY&v=1`, 20 a page |
| `latest` | the same with `sort=POST_DATE` (newly posted anime) |
| `search` | `GET /search?page=N&q=...&v=1` |
| `details` | `GET /anime/{slug}`: summary, genres, poster |
| `episodes` | `GET /anime/{id}/episode`, newest first, named "Ep. N - Title" as the Kotlin does |
| `hosters`, `videos` | `GET /ep/{uid}?origin={id}`: one hoster whose video is `https://stream.animeparadise.moe/m3u8?url={streamLink}` (HLS), with every subtitle the site lists (`/stream/file/{src}`, mostly `.ass`) |

Adult titles (age rating R18) follow the app's **Show adult (18+) titles** switch (`showAdult`): they are left out of the
lists while it is off.

## Not ported

- The genre filter and the quality setting (the site serves one HLS playlist; the player picks the quality).
