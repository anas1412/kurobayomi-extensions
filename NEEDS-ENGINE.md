# Features the recipes need from the engine

## manga/gwtb

Missing: a way to build a link out of a value that is not a link. Proposed: an optional `format` key on a value (after
`regex`), where `{}` is replaced by the value: `url = { attr = "value", format = "/index.php?nro={}" }`.

Why: the strips of Gone with the Blastwave are only listed as `<option value="93">#81 Knife Fight</option>` inside the
"Jump to a strip" select. The strip address is `/index.php?nro=93` (the keiyoushi extension builds it the same way). The
recipe can read `93` but not turn it into a link, so the engine joins it to the page and stores `/93`, which is a 404.
No other element of the page links to the strips (only first and previous do).

With `format` the chapters table of `manga/gwtb/recipe.toml` becomes `url = { attr = "value", format = "/index.php?nro={}" }`
and nothing else changes. Checked: the page `https://www.blastwave-comic.com/index.php?nro=92` is a normal strip page
(saved as `manga/gwtb/fixtures/chapter.html`), so the pages operation already works on it.

## manga/existentialcomics

Nothing missing. (Small workaround: the extension has the entry built in, the recipe reads it from the site's category
archive page, because that is the only page that links to the by-date archive. A fixed-value option would make this
simpler but is not needed.)

## anime/beatzanime

Three things are missing. The first two stop playback; the third stops search.

1. **Cross-host links are stored without their host.** The episode links are direct files on another host,
   `https://bz.beatz-anime.net/d/BeatZ-Anime/Anime/<name>/<file>.mkv`. `stored()` keeps only the path, so the episode is
   saved as `/d/BeatZ-Anime/...` and later joined to `www.beatz-anime.net`, a 404 (the checker's live run shows
   `videos: HTTP error 404`). Proposed: keep the full address when the host differs from the first `base_urls` host.
2. **The episode link is already the video, but `videos` always fetches a page.** Proposed: a key on `[videos]`, for
   example `from_episode_url = true`: no request, the stored episode address (made absolute) is the one video. Its
   title comes from `title` read from the episode name or `default_title`. The Kotlin names the video by the `1080p`
   found in the file name; a `regex` on that name would do. The file host must be allowed to hold the file without being
   requested as a page (it is only handed to the player), and the recipe `headers` (Referer) go with it, as now.
3. **Search is filtered in the page's JavaScript.** `https://www.beatz-anime.net/lista-animes/` returns all anime
   (about 440 KB) and ignores any query; each `div.anime-card` has `data-name` (lower case, no accents). The recipe has
   no `search` at all, because returning the whole list for every query would be wrong. Proposed: an optional
   `filter = { attr = "data-name", contains = "{query}" }` on `[search]` that keeps the items whose value, lower-cased
   and with accents removed, contains the query (also lower-cased and unaccented).

Also not covered, minor: the Kotlin puts specials (OVA, NCOP) after the regular episodes and numbers them -1; the recipe
only has `reverse`. The portrait poster of the latest cards is built from the slug in the Kotlin; the recipe uses the
16:9 still from the card.

## anime/kimoitv

Playback cannot be done by a recipe, and the reason is the site, not only the engine.

1. **The video request is a POST behind a bot check.** The Kotlin reads `div#fileInfo[data-id][data-name]` on the
   episode page (`/download/<id>/<name>.html`), POSTs `d=<data-name>&id=<data-id>` to `/streamvpaid.php` and reads
   `<source src>` from the answer. Checked on 2026-10-10 with the same POST (Referer set to the episode page,
   `X-Requested-With: XMLHttpRequest`): the answer is the text `Invalid Ticket! Reload this page and wait 5 second to
   load Player.` The episode page now loads `https://challenges.cloudflare.com/turnstile/v0/api.js` (Cloudflare
   Turnstile), so the ticket is issued by script after the check. That is a bot check; it is out of scope ("a Cloudflare
   challenge" is already on the not-in-this-version list). Even with POST support the source would not play.
   Needed for the part that is possible: requests with a body (`[videos] post = { path = "/streamvpaid.php", body =
   "d={data-name}&id={data-id}" }` taking the two values from the page).
2. **Episodes are one page level deeper than a recipe reads.** The title page (`/title/<id>/<name>.html`) lists only the
   seasons, as links `/title/<id>/<name>/Season-01.html` and `.../Season-01-DUB.html` (`div#seasonsAccordion
   a.list-group-item`). The episode links (`ul.link-listview > li > a`, `/download/<id>/<name>-S01E366.html`) are on
   those season pages, 20 or more per page and paginated by `ul.pagination` (the Kotlin follows the pages in a loop).
   Proposed: an `[episodes] follow = { items = "...", next_page = "..." }` that fetches each linked page and appends
   its `items`, with the follow link's text available to `name` as a prefix. Until then the recipe lists the seasons
   as if they were episodes (a stopgap, nothing under them plays).
3. **Search** returned "No Search Result" for `naruto` (live checker), `bleach` and `bleac` (by hand), though the same
   titles are in the lists. The page offers `/search_v2/` and the text says to type a word prefix. Not investigated
   further (request budget used), so the recipe has no `search`.

The site's markup also changed since the Kotlin was written (`ul.media > li` is now `div.movie-grid > div.content-card`);
the recipe uses the current markup.
