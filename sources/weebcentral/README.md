# Weeb Central (`weebcentral`)

Manga source for [Weeb Central](https://weebcentral.com), as a WASM plug-in for Kurobayomi (ABI 1.5, `wit/kuro-source.wit`, package `kuro:source@1.5.0`).

## Licence and origin

Ported from the keiyoushi `en/weebcentral` extension (version code 25), which is licensed under the Apache License 2.0.
This plug-in is a port of the Rust crate that lived in the app repository (the same logic, in the same license).
Apache License 2.0: https://www.apache.org/licenses/LICENSE-2.0

## What it implements

- Ops: `info`, `create-sources`, `popular`, `latest`, `search`, `details`, `chapters`, `pages`.
- Popular (`sort=Popularity`), latest (`sort=Latest Updates`), and search by text, 32 entries a page. A "View More
  Results" button means another page.
- Search filters (ABI 1.5 `filters`), read by their Kotlin names: Sort, Sort Order, Official Translation, Anime
  Adaptation, Adult Content, Author (Case-sensitive), Series Status, Series Type and Tags (include and exclude). An
  absent filter takes its Kotlin default. The app holds no filters today, so a search sends the defaults.
- Request headers (ABI 1.5 `request-headers`): `Accept: image/avif,image/webp,*/*`, and no Origin, as the Kotlin
  `imageRequest` sends.
- Spacing (ABI 1.5 `requestGapMs`): 2000 ms between two requests to the site, as the Kotlin `rateLimit(1, 2.seconds)`.
- Details: title, author, genres with type, status, cover, description with related series, alternate names and tracker
  links.
- Chapters: newest first, dates (epoch milliseconds, read from the page's `datetime`), "Official" or "Unknown" scanlator,
  position numbering for names with a season.
- Pages: the long-strip page images, in order.

## What changed in ABI 1.5 (version 2)

- The 2 second spacing is the source's `requestGapMs`, enforced by the host. Before, nothing rate-limited the site.
- The search filters are read from the app's filter list (see above). Before, every search used the defaults.
- Page images send the `Accept` header of the Kotlin (`request-headers`). Before, no image headers were sent, so an
  image host that wanted a Referer would fail. Page images are still fetched by the app.
- Links are resolved against the final address of each page, after redirects (`http` reply `url`), as the Kotlin
  resolves them against `document.location()`. Before, the requested address was used.
- Filters, headers and dates are tested in `src/site.rs` and `src/lib.rs` (`cargo test`).

## What is not ported

- Deep links (`/series/{id}/{slug}`): no URI op in the ABI.
- Related series and recommendations (the Kotlin `fetchRelatedMangaList`): the ABI has no related-titles op. The
  related series appear in the description, as before.
- The Kotlin `getMangaByUrl`: the same ABI gap.
- The `Referer` of the API calls (`https://weebcentral.com/`) is kept. The Kotlin client adds no Referer; it is not
  known whether the site needs it.

## Files

- `src/site.rs`: URLs, filters and parsers (pure, with the tests over `fixtures/`).
- `src/lib.rs`: the ABI glue, wire shapes, `request-headers`, `info`, and the `http` call.
- `fixtures/`: pages saved from the site on 2026-10-10.
- `manifest.toml`: id, name, language, kind and the host allow-list.

Size: 942,427 bytes for ABI 1.4 (cap 1,048,576); the ABI 1.5 build is not measured yet. Built for wasm32-unknown-unknown, no WASI imports.

## Check

```
cargo test --offline -j 1          # the parsers over the saved pages, on the host
tools/build.sh                     # from the repository root
```

The id is `2131019126180322627`, the same as `generate_id("Weeb Central", "en", 1)` gives in the app.

The live site was not reached from the sandbox, so the plug-in has not been run against weebcentral.com. The build was
not run for ABI 1.5 either (the lead builds).
