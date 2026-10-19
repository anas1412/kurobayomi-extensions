# AnimeGG

AnimeGG (<https://www.animegg.org>) as an anime source plug-in for Kurobayomi (ABI 1.5, `wit/kuro-source.wit`, anime ops),
English: subbed, dubbed and raw.

## Licence and origin

Licensed under the Apache License 2.0. Ported from the Kohi-den `en/animegg` extension (Apache-2.0, version 1). The page
selectors are the Kotlin's, checked against the live site. The source id is the Kotlin one: `MD5("animegg/en/1")` taken
as its first 8 bytes, big-endian, with the top bit cleared: `2247987965040486818`. Anime and episode urls are the
site's paths (`/series/naruto`, `/naruto-episode-1`), as the Kotlin keeps them.

## Ops

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; Latest; 300 ms between requests |
| `request-headers` | `Referer: https://www.animegg.org` (the video host answers 500 without it) |
| `popular` | `/popular-series?sortBy=hits` (one page, as the site lists it) |
| `latest` | `/popular-series?sortBy=createdAt` |
| `search` | `/search/?q=...`; no text shows the popular list, as the Kotlin does |
| `details` | the series page: title, summary, genres, poster, status |
| `episodes` | the series page's list, newest first; "Episode N - Title"; the versions (SUBBED, DUBBED, HD) as the scanlator |
| `hosters` | the episode page's player tabs, `[SUBBED]` first (the Kotlin's default language) |
| `videos` | the player page's `videoSources`: MP4 files on the site, which redirect to its video host, best quality first |

## Not ported

- The genre filter (its two options were the site's Spanish film and series lists) and the settings (preferred
  language, server and quality: subbed and the best quality come first).
