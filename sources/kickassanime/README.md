# KickAssAnime

KickAssAnime (<https://kaa.lt>) as an anime source plug-in for Kurobayomi (ABI 1.4, anime ops).

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
| `search` | a `slug:` or kaa.lt link: details of that show. Empty text: `GET /api/anime?page=N`. Text: `POST /api/fsearch` |
| `episodes` | `GET /api/show/{slug}/language`, then `/episodes?page=N&lang=L` for the first language with episodes (ja-JP, en-US, then the site's order), newest first |
| `hosters` | `GET /api/show/{slug}/episode/ep-{n}-{id}`: one hoster per server, the preferred server (VidStreaming) first |
| `videos` | the server's player page, then its manifest (HLS master or DASH MPD), split per quality with subtitles and audio tracks |
| `subtitles` | the subtitle tracks of a video |

Each video carries its own `headers` (Origin of the player host, no Referer, a mobile Chrome user agent). The app's
player sends them with the playlist, the segments and the subtitles. The guest sends the same headers with the manifest.

## Changes from the Rust port

- Not ported: the browse filters (genre, year, status, type, sub-page). The app sends no filters to anime packages,
  and the contract has no filter list.
- Not ported: the older `cid` player (SHA-1 signed `source.php`, AES-256-CBC answer). Every server seen on 2026-10-10
  used the current player. A server with the older player gives no videos and a warning in the log.
- Not ported: the English-title setting and the other Kotlin settings. They stay at their defaults, so the English
  branch is gone.
- Not an op: anime details. The contract has no anime `details` op, so the synopsis, dub list and season come only with
  a search by slug or link.
- Dropped the speed term of the video sort. Its pattern (`KB/s`) never matches the titles this source builds.
- No regex and no chrono: the HLS attribute reader splits on commas outside quotes, and the year list is gone with the
  filters.

## Hosts

`manifest.toml` allows `kaa.lt`, `krussdomi.com` and `*.krussdomi.com`. The guest fetches the API, the player pages and
the manifests (`hls.krussdomi.com`, `bl.krussdomi.com`). The player fetches the variant playlists, segments and subtitles
itself, so those hosts need no entry. A server whose manifest is elsewhere fails with `host not allowed`.

## Checks

- `cargo test` in this folder: the pure logic over the saved responses in `fixtures/` (no network).
- Not verified here: the built component running in the app, and the live site. The sandbox has no route to kaa.lt in
  the app's host, so the request and response shapes follow the contract and the app's `bridge.rs`, not a live run.

## Build

```
tools/build.sh   # from the repository root; builds to dist/kickassanime.wasm
```

The output is `dist/kickassanime.wasm`, a component with no WASI imports. The index links that file.
