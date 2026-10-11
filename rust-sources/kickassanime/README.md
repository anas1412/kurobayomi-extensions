# kuro-src-kickassanime

KickAssAnime (<https://kaa.lt>) as a built-in anime source, ported from the yuzono `en/kickassanime` extension
(Apache-2.0, `~/ext-refs/yuzono_anime-extensions/src/en/kickassanime`).

## Register it

```rust
let kaa: Arc<dyn AnimeSource> = Arc::new(kuro_src_kickassanime::source(network_client.clone()));
```

Id `2081771513481539599` (`generate_id("KickAssAnime", "en", 1)`), the same as the Kotlin extension, so library
entries from an Aniyomi backup match it.

## What it does

| Operation | Request |
|---|---|
| popular | `GET /api/show/trending?page=N` |
| latest | `GET /api/show/recent?type=all&page=N` |
| search, text | `POST /api/fsearch {"page":N,"query":"..","filters":"<base64>"}` |
| search, filters only | `GET /api/anime?page=N&filters=<base64 JSON>` (genres, year, status, type) |
| search, Sub-page filter | `GET /api/{show/trending, anime, show/recent, show/popular}?page=N` |
| search `slug:x` or a kaa.lt link | details of that one show |
| details | `GET /api/show/{slug}` + `/language` |
| episodes | `GET /api/show/{slug}/episodes?page=N&lang=L`, first language with episodes (ja-JP, en-US, then the site's order) |
| hosters | `GET /api/show/{slug}/episode/ep-{n}-{id}`: one hoster per server, loaded through `videos_for_hoster` |
| videos | the player page, then the HLS master or DASH MPD split per quality, with subtitles and audio tracks |

Each `Video` carries its own `headers` (Origin of the player host, no Referer, the Kotlin mobile Chrome user agent).
The player sends them with the playlist, the segments and the subtitles. Segments are served as `.jpg` files on
rotating hosts; a local test showed mpv 0.41 plays such a playlist without extra options, so no `mpv_args` are set.

## What changed since the Kotlin

- Domain: `kaa.lt` is still the primary domain and the API paths are unchanged.
- Players: every server seen on 2026-10-10 (VidStreaming, BirdStream) uses `krussdomi.com/cat-player/player`, an
  Astro page whose island `props` hold the manifest and subtitles. This crate decodes the Astro props as JSON instead of
  the Kotlin regexes. The older `cid` player (SHA-1 signed `source.php` call and AES-256-CBC answer) is ported too
  but was not seen live; it is tested over vectors made with openssl and Python.
- Search by slug: the Kotlin stored `/api/show/{slug}` as the anime URL there, which breaks the next details call; this
  crate stores `/{slug}`.
- An audio language the Kotlin list does not name (for example `es-419`) is shown as its code, not as an empty string.

## Not done

- Settings: the Kotlin preferences (English titles, audio languages, quality, server, domain, excluded hosts) are
  fixed at their defaults in `lib.rs`. The app has no settings store for built-in sources yet; when it does, implement
  `ConfigurableSource` and read them.
- Related anime (Kotlin reads `window.KAA` from the show page): not ported.
- CatStream subtitle repair (Kotlin `fixSubtitles` writes cleaned VTT files): only reached by the older player's
  CatStream, which has no key and so never gives videos; not ported.

## Checks

- `cargo test -p kuro-src-kickassanime`: every operation over the responses saved in `tests/fixtures` (no network).
- `cargo run -p kuro-src-kickassanime --example live_check [query]`: popular, latest, search, details, episodes,
  hosters and videos against the real site, one request a second.
