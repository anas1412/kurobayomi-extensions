# Gone with the Blastwave

A webcomic about war, by Kimmo Lemetti: `https://www.blastwave-comic.com`. English. The whole site is one comic, so Browse
shows one entry; its chapters are the strips (one image each).

Ported from the keiyoushi extension `src/en/gwtb` (https://github.com/keiyoushi/extensions-source).
Original work: Copyright the keiyoushi contributors, Apache License 2.0. This recipe is a changed version: the Kotlin
code was turned into a text recipe (CSS selectors and URL templates). Changes: the entry, author and description are read
from the home page instead of being built in (so there is no cover); the strip list is read from the "Jump to a strip"
select.

## Install

Copy `recipe.toml` into the scrapers folder of the app as `gwtb.toml` (the app reads every `*.toml` there, so each source
needs its own file name) and restart the app.

## Check

```
cargo run -p kuro-source-scrape --example check_recipe -- recipe.toml fixtures/
cargo run -p kuro-source-scrape --example check_recipe -- recipe.toml fixtures/ --live
```

Fixtures (saved 2026-10-10, unchanged): `index.html` (`/index.php`), `chapter.html` (`/index.php?nro=92`).
