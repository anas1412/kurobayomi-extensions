# Existential Comics

A philosophy webcomic by Corey Mohler: `https://existentialcomics.com`. English. The whole site is one comic, so Browse
shows one entry; its chapters are the comics (675 in the by-date archive when this was saved), newest first. A comic can
have several images, they are its pages.

Ported from the keiyoushi extension `src/en/existentialcomics` (https://github.com/keiyoushi/extensions-source).
Original work: Copyright the keiyoushi contributors, Apache License 2.0. This recipe is a changed version: the Kotlin
code was turned into a text recipe (CSS selectors and URL templates). Changes: the entry and its description are read
from the site pages instead of being built in (the cover is the site logo, read from the page; there is no author, the extension had it built in and no page of the site names it); the Jsoup `a:eq(0)` selector is
`nth = 0`; the chapter list is reversed so the newest comes first.

## Install

Copy `recipe.toml` into the scrapers folder of the app as `existentialcomics.toml` (the app reads every `*.toml` there, so
each source needs its own file name) and restart the app.

## Check

```
cargo run -p kuro-source-scrape --example check_recipe -- recipe.toml fixtures/          # saved pages
cargo run -p kuro-source-scrape --example check_recipe -- recipe.toml fixtures/ --live   # the real site
```

Fixtures (saved 2026-10-10, unchanged): `archive-category.html` (`/archive`), `archive-bydate.html` (`/archive/byDate`),
`comic.html` (`/comic/1`).
