# kuro-src-weebcentral

Built-in manga source for Weeb Central (https://weebcentral.com), ported from the keiyoushi `en/weebcentral`
extension (Apache-2.0, version code 25). Plain HTML and htmx fragments, no crypto, no descrambling.

## What it implements

- `MangaSource`, `CatalogueSource` and `ResolvableSource` on `WeebCentral`.
- Popular (`sort=Popularity`), latest (`sort=Latest Updates`), search with text and every Kotlin filter: sort, order,
  official translation, anime adaptation, adult content, author, series status, series type, 38 tags (include and
  exclude). 32 entries per page, `offset = (page - 1) * 32`; a "View More Results" button means another page.
- Details (title, author, genres plus type, status, cover, description with related series, alternate names and
  tracker links), the full chapter list (dates, "Official" or "Unknown" scanlator, position numbering for season
  names), and the long-strip page images.
- Deep links: `https://weebcentral.com/series/{id}/{slug}` resolves to the manga.
- Requests go to `https://weebcentral.com` only, one at a time, two seconds apart (Kotlin `rateLimit(1, 2.seconds)`),
  with the app user agent and `Referer: https://weebcentral.com/`. No `Origin` header (the Kotlin removes it).
- The id is `generate_id("Weeb Central", "en", 1)` = `2131019126180322627`, the same as the Kotlin defaults give, so
  existing library entries from an Aniyomi or Mihon backup should match. Check against a real backup.

Not ported: related manga (`fetchRelatedMangaList`), because `kuro-source` has no related-manga trait.

## What the lead must wire

1. Register it: `kuro_src_weebcentral::source(client)` takes the app's `NetworkClient` and returns `WeebCentral`;
   wrap it in `Arc` as a `dyn CatalogueSource` (and `dyn ResolvableSource` for deep links).
2. Page images are on another host (`temp.compsci88.com`, covers too). The Kotlin `imageRequest` sends
   `Accept: image/avif,image/webp,*/*` and the Referer; `WeebCentral::image_headers()` returns those pairs for the
   image loader and the chapter downloader. They load without them today too, so this is a nicety, not a blocker.

## Proof

- `cargo test -p kuro-src-weebcentral`: every operation over pages saved from the site on 2026-10-10
  (`tests/fixtures`; the chapter list is cut to 11 of 1,195 entries).
- `cargo run -p kuro-src-weebcentral --example live_check`: popular, latest, search, details, chapters and pages
  against the real site, exits non-zero when a step fails or returns nothing.
