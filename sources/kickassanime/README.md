# KickAssAnime

KickAssAnime (<https://kaa.lt>) as an anime source plug-in for Kurobayomi (ABI 1.5, `wit/kuro-source.wit`, anime ops).

## Licence and origin

Licensed under the Apache License 2.0. Ported from the yuzono `en/kickassanime` extension for Aniyomi
(Apache-2.0, `src/en/kickassanime`), through the built-in Rust port that lived in the app repository.
The API paths, the response shapes and the player handling follow that port; the source id is the Kotlin one.

## Ops

| Op | Request |
|---|---|
| `info`, `create-sources` | the descriptor, id `2081771513481539599` (`MD5("kickassanime/en/1")`, the Kotlin id) |
| `popular` | `GET /api/show/trending?page=N` |
| `latest` | `GET /api/show/recent?type=all&page=N` |
| `search` | a `slug:` or kaa.lt link: details of that show. A Sub-page filter: `GET /api/{sub-page}?page=N`, which replaces the rest. Empty text: `GET /api/anime?page=N`, with `&filters=` when a filter is set. Text: `POST /api/fsearch`, with `filters` when set |
| `request-headers` | the video headers without Origin (ABI 1.5) |
| `episodes` | `GET /api/show/{slug}/language`, then `/episodes?page=N&lang=L` for the first language with episodes (ja-JP, en-US, then the site's order), newest first |
| `hosters` | `GET /api/show/{slug}/episode/ep-{n}-{id}`: one hoster per server, in the site's order |
| `videos` | the server's player page, then its manifest (HLS master or DASH MPD), split per quality with subtitles and audio tracks |
| `subtitles` | the subtitle tracks of a video |

Each video carries its own `headers` (Origin of the player host, no Referer, a mobile Chrome user agent). The app's
player sends them with the playlist, the segments and the subtitles. The guest sends the same headers with the manifest.

## ABI 1.5 (version 2)

- **Filters.** `search` reads the app's filter list by the Kotlin names (`KickAssAnimeFilters.kt`): Genre (the checked
  genres), Year, Status, Type and Sub-page. The Kotlin text of the filters (`{"genres":[...],"year":N,"status":"x",
  "type":"y"}`) is sent base64 in `filters` (the query of a browse, the body of a text search), as the Kotlin does.
  A Sub-page other than "<Select>" replaces the search and the other filters, as in the Kotlin, and its answer is read by
  the endpoint's shape.
- **Request headers.** `request-headers` answers the player's headers without the Origin: `Accept`, `Accept-Language`,
  the `Sec-Fetch-*` headers and the mobile user agent. The Origin is the player's own host, and each video already carries
  its Origin in `headers`, so the app's per-video headers keep the rule. No `Referer`.
- **Episode videos sorted.** The videos of an episode come out in the Kotlin `sortVideos` order (the preferred server,
  the preferred quality, the height), as the hoster videos already did. Hosters keep the site's order, as in the Kotlin.
- **Languages.** A code the Kotlin list does not name has the empty label (Kotlin `getLocale`), in the dub list too, so
  the description shows an empty entry for such a code.
- **Sort ties.** Videos with equal keys come out in reverse order, as the Kotlin `sortedWith(...).reversed()` does.
- **No spacing.** The Kotlin sets no rate limit for this source, so `info` carries no `requestGapMs`.

## Changes from the Rust port

- Filters: ported in ABI 1.5 (see above). The app holds no filters today, so a search sends none.
- Not ported: the older `cid` player (SHA-1 signed `source.php`, AES-256-CBC answer). Every server seen on 2026-10-10
  used the current player. A server with the older player gives no videos and a warning in the log.
- Not ported: the English-title setting and the other Kotlin settings. They stay at their defaults, so the English
  branch is gone.
- Deliberate difference: a link or `slug:` search stores the show as `/{slug}`, as a listing does. The Kotlin stores the
  request URL of the show's API (`/api/show/{slug}`), and its episode lookup then builds a doubled path and fails.
- Not an op: anime details. The contract has no anime `details` op, so the synopsis, dub list and season come only with
  a search by slug or link.
- Dropped the speed term of the video sort. Its pattern (`KB/s`) never matches the titles this source builds.
- No regex and no chrono: the HLS attribute reader splits on commas outside quotes. The base64 of the filters is written
  here (no crate).

## Hosts

`manifest.toml` allows `kaa.lt`, `krussdomi.com` and `*.krussdomi.com`. The guest fetches the API, the player pages and
the manifests (`hls.krussdomi.com`, `bl.krussdomi.com`). The player fetches the variant playlists, segments and subtitles
itself, so those hosts need no entry. A server whose manifest is elsewhere fails with `host not allowed`.

## Checks

- `cargo test` in this folder: the pure logic over the saved responses in `fixtures/` (no network). The ABI 1.5 tests
  cover the filter text and base64, the sub-pages, the request headers and the sort order. Not run in this change (the lead
  builds).
- Not verified here: the built component running in the app, and the live site. The sandbox has no route to kaa.lt in
  the app's host, so the request and response shapes follow the contract and the app's `bridge.rs`, not a live run.

## Build

```
tools/build.sh   # from the repository root; builds to dist/kickassanime.wasm
```

The output is `dist/kickassanime.wasm`, a component with no WASI imports. The index links that file.
