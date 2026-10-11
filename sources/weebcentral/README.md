# Weeb Central (`weebcentral`)

Manga source for [Weeb Central](https://weebcentral.com), as a WASM plug-in for Kurobayomi (ABI 1.4, `wit/kuro-source.wit`).

## Licence and origin

Ported from the keiyoushi `en/weebcentral` extension (version code 25), which is licensed under the Apache License 2.0.
This plug-in is a port of the Rust crate that lived in the app repository (the same logic, in the same license).
Apache License 2.0: https://www.apache.org/licenses/LICENSE-2.0

## What it implements

- Ops: `info`, `create-sources`, `popular`, `latest`, `search`, `details`, `chapters`, `pages`.
- Popular (`sort=Popularity`), latest (`sort=Latest Updates`), and search by text (`sort=Best Match`), 32 entries a page.
  A "View More Results" button means another page.
- Details: title, author, genres with type, status, cover, description with related series, alternate names and tracker
  links.
- Chapters: newest first, dates, "Official" or "Unknown" scanlator, position numbering for names with a season.
- Pages: the long-strip page images, in order.

## What changed from the Rust crate

- The network calls go through the host's `http` import (`host::http`), not the app's `NetworkClient`. The host is meant to refuse
  any host outside the allow-list (`weebcentral.com`), but the app does not read this `manifest.toml`; see the root README.
- The 2 second spacing between requests is not in the guest (no clock or sleep import in ABI 1.4). Nothing rate-limits
  today: `kuro-net` has `rate_limit_host`, but the wasm host does not configure it. App side: set it to 1 request per 2 s for
  `weebcentral.com`. Until then bursts over chapters or pages risk 429 or Cloudflare blocks.
- The search filters (sort, status, type, tags, author) are not ported: the ABI 1.4 `search` op carries only `page`
  and `query`, and the app sends `filters: []`. Search uses the Kotlin defaults.
- Deep links (`/series/{id}/{slug}`) are not ported: ABI 1.4 has no URI op.
- The image headers (`Referer`, `Accept`) for page images are not sent: the app loads images, and ABI 1.4 has no
  header field for them. Wasm sources get no image headers in the app (only recipes do, `recipe_sources.rs`), so page images fail if the image host
  requires a Referer. Unverified: no live test.
- Links are kept in their encoded form (`url::Url` path, query and fragment). The Rust crate decodes them. Weeb Central's
  paths are ULIDs and plain slugs, so the two forms are the same for the site today.
- The chapter and series dates are read with `chrono`'s RFC 3339 parser, as before.

## Files

- `src/site.rs`: URLs and parsers (pure, with the tests over `fixtures/`).
- `src/lib.rs`: the ABI glue, wire shapes and the `http` call.
- `fixtures/`: pages saved from the site on 2026-10-10.
- `manifest.toml`: id, name, language, kind and the host allow-list.

Size: 942,427 bytes (cap 1,048,576), built for wasm32-unknown-unknown, no WASI imports.

## Check

```
cargo test --offline -j 1          # the parsers over the saved pages, on the host
tools/build.sh                     # from the repository root
```

The id is `2131019126180322627`, the same as `generate_id("Weeb Central", "en", 1)` gives in the app.

The live site was not reached from the sandbox, so the plug-in has not been run against weebcentral.com.
