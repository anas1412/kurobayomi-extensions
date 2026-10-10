# KimoiTV (`kimoitv`) - PARKED, not part of the first set

English site (`https://kimoitv.com`) with anime, series, films and drama as downloadable files.

Ported from the Aniyomi extension `src/en/kimoitv` of
[yuzono/anime-extensions](https://github.com/yuzono/anime-extensions), which is licensed under Apache-2.0. The Kotlin
code was changed into a declarative recipe (selectors and URL templates, no code). The site's markup has changed since
that extension was written, so the selectors are new. Licensed under Apache-2.0 as well.

This folder sits under `anime/_parked/` on purpose: the source cannot play video or list real episodes, so it is not
shipped. Do not copy `recipe.toml` into a `scrapers` folder. It is kept so the work is not lost if the site or the
engine changes.

## What works

- Popular (sorted by popularity) and Latest (recently added), with paging.
- Details: cover, description, genres.

## What does not work, and why

- Playback. Checked live on 2026-10-10 (one request each, polite user agent): the episode page
  `/download/660273/Bleach-S01E366.html` loads `https://challenges.cloudflare.com/turnstile/v0/api.js`, and the POST
  the Kotlin sends to `/streamvpaid.php` (`d=2&id=660273`, Referer set to the episode page) answers `Invalid Ticket!
  Reload this page and wait 5 second to load Player.` The ticket is issued by script after a Cloudflare Turnstile
  check. That is a bot check, so it is not something to work around. (A recipe also has no POST.)
- Episodes. The title page lists only the seasons; the episode links are on the season pages
  (`season.html`, paginated). A recipe reads one page, so the `[episodes]` block lists the seasons as if they were
  episodes. It is a stopgap that only exists because the recipe format requires `[episodes]` for an anime. Nothing
  under those entries plays.
- Search. Checked live on 2026-10-10: `/search/?q=naruto` answers 302 to `/verify/?q=naruto&page=1`, a page titled
  "Verify Request" that loads the same Turnstile script and only continues after the script posts a token back. The
  302 response body itself reads "No Search Result was found for naruto". `/search_v2/?q=naruto` behaves the same. So
  the earlier "no result" was this gate, not a missing query syntax, and search cannot be done without passing the
  check.
- Genre and A-Z filters are not ported.

## Fixtures

`fixtures/` holds saved pages (style and script blocks removed, otherwise unchanged): `popular.html`, `latest.html`,
`title.html` (details and seasons), `season.html` (an episode list; no operation reads it) and `episode.html` (the
episode page). The Turnstile finding above is NOT backed by these files: their script blocks are empty, so
`episode.html` contains no Turnstile reference. It comes from the live checks quoted above.

## Tests

```
cargo run -p kuro-source-scrape --example check_recipe -- anime/_parked/kimoitv/recipe.toml anime/_parked/kimoitv/fixtures
```

from the Kurobayomi repository. Popular, latest, details and the stopgap episode list pass; `videos` fails (no fixture
is named for it and no video address can be read), so the checker exits 1. That is the truth about this source.
