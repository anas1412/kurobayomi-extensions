# BeatZ Anime (`beatzanime`)

Spanish fansub site (`https://www.beatz-anime.net`) that lists BDRip and WebRip releases of series and films, with the
files hosted on its own file host.

Ported from the Aniyomi extension `src/es/beatzanime` of
[yuzono/anime-extensions](https://github.com/yuzono/anime-extensions), which is licensed under Apache-2.0. The Kotlin
code was changed into a declarative recipe (selectors and URL templates, no code). Only the behaviour the recipe format
can express was carried over. Licensed under Apache-2.0 as well.

## What works

- Popular: the "top 7 most viewed" strip of the home page.
- Latest: the recent uploads of the home page, with paging.
- Details: title, cover, description, genres, status (finalizado / en emision).
- Episode list: the rows of the "Lista de Archivos" table, newest first. Each episode holds the address of the file.

## What does not work yet

- Playback. Not fixable in a recipe, it needs two engine changes (details in `NEEDS-ENGINE.md`, anime/beatzanime):
  keep a link's host when it differs from the page host, and a `[videos]` option that takes the stored episode address
  as the video. Checked live on 2026-10-10 (polite user agent): the file link
  `https://bz.beatz-anime.net/d/BeatZ-Anime/Anime/Jujutsu%20Kaisen/...01...mkv` answers a ranged GET with 206
  `video/x-matroska` when the Referer is `https://www.beatz-anime.net/` (so the file itself is fine for the player),
  while the same path on `www.beatz-anime.net`, where the stored link lands today, is a 404. Until the engine changes
  land this source browses but does not play, and `check_recipe` exits 1 on the `videos` step.
- Search: the site filters in JavaScript, there is no server search, and `/lista-animes/` returns all 440 KB of anime
  whatever the query. The recipe has no search until the engine can filter a listing by the query (`NEEDS-ENGINE.md`).
- Episode order: the Kotlin puts specials (OVA, Especial, NCOP) after the regular episodes. The recipe lists the rows as
  the site does, reversed. A row whose name does not end in a number (an NCOP, say) gets episode number -1 (the engine default),
  but an "OVA 1" row still gets 1, because the recipe cannot detect specials by name. The ordering, the -1 for all
  specials and the movie-page rename need an engine feature. Not checked against the live site: no special rows are in
  the saved fixtures.
- Genre filters are not ported.

## Install

Copy `recipe.toml` into the `scrapers` folder of the app (the `scrapers` folder inside its data directory) as
`beatzanime.toml`, then restart the app.

## Tests

`fixtures/` holds saved pages of the site (style and script blocks removed, otherwise unchanged): `home.html`
(popular and latest) and `jujutsu.html` (details and episodes). Run

```
cargo run -p kuro-source-scrape --example check_recipe -- anime/beatzanime/recipe.toml anime/beatzanime/fixtures
```

from the Kurobayomi repository (add `--live` to run against the site). The `videos` step fails by design until the
engine features above exist.
