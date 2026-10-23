# InkStory

InkStory (<https://inkstory.net>) as a manga source plug-in for Kurobayomi (ABI 1.5, `wit/kuro-source.wit`, manga ops),
Russian only.

## Licence and origin

Licensed under the Apache License 2.0. Ported from the keiyoushi `ru/inkstory` extension and the `inkstory` theme it
builds on (`lib-multisrc/inkstory`, Apache-2.0). The API paths, the headers, the title, chapter and page rules and the
image key follow it. The source id is the Kotlin one: `MD5("inkstory/ru/2")` (the extension's `versionId = 2`) taken as
its first 8 bytes, big-endian, with the top bit cleared: `2026387314200968454`. Manga urls are the book slugs and chapter
urls the chapter ids, as in the Kotlin, so Aniyomi backups match.

## Ops

All requests go to the shared API `https://api.inuko.me/v2` with the Kotlin's `User-Agent` and `X-Service-Name: inkstory`.

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor; Latest |
| `popular` | `GET /books?serviceName=inkstory&sort=viewsCount,desc`, 30 a page; the next page from `x-estimated-total-hits` |
| `latest` | `GET /chapter-update-feed?onlyBorderChapters=true`, the books of the latest chapter updates |
| `search` | `GET /books?search=...`, most viewed first. No filters |
| `details` | `GET /books/{slug}`: summary, rating, views, likes, bookmarks and links in the description; labels and formats as genres; authors and artists; status (ongoing, done, frozen) |
| `chapters` | `GET /books/{slug}` for the book id, then `/branches` and `/chapters` (approved only): every branch, newest first; a branch's translators are the scanlator |
| `pages` | `GET /chapters/{id}`: the page images in order; an `s` (server cipher) image is asked for as its `x` twin, as the Kotlin does |
| `unscramble` | ABI 1.5 image hook: an `x` page image is XOR-ciphered with a fixed key; it is read back when its bytes do not start like an image and the deciphered ones do (JPEG, PNG, GIF, WebP, AVIF). Anything else is shown as fetched |

Covers and page images (`static.inuko.me`) load in the app and need no headers, so the manifest lists only
`api.inuko.me`. No login and no Cloudflare check; from outside Russia the API answered without a block on 2026-10-23.
The site has no adult (`EROTIC`) titles, so the app's adult switch changes nothing here.

## Not ported

- The filters (genres, strict labels, status, country, content status, format, rating, year and chapter-count ranges,
  sort order) and the deep link.
- The settings: the branch mode ("latest version of a chapter" and "preferred team" instead of every branch) and hiding
  chapters newer than 4 days. The plug-in always lists every branch, the Kotlin's default.

## Tests

`cargo test` reads the fixtures, saved from the live API on 2026-10-23 and trimmed: a book list, the update feed, a book,
its branches, a few chapters, a page list and the first 64 bytes of a ciphered page image.
