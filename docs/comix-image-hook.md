# Comix page images: the hook the app must add

Comix serves some page images XOR-encoded, or cut into a shuffled 5x5 grid. The image host says which in the response
headers (`x-enc-seed`, `x-enc-len`, `x-enc-algo`, `x-scramble-grid`, `x-scramble-seed`, `x-scramble-algo`,
`x-scramble-hash`). The plug-in can restore them, but the app's reader does not reach that code today.

## Why a hook is needed

The reader loads a page image in `~/kurobayomi/src/reader/source_pages.rs`: `SourcePages::image` calls `fetch`, which
returns only the body bytes. The response headers are dropped, and nothing gives the bytes to the source. The WIT
contract (`crates/kuro-source-wasm/wit/kuro-source.wit`) has no op for image fix-up either.

The hook is one more op on the existing `invoke` export, additive like the anime ops. A plug-in without it answers
`unknown op: unscramble`, which the app must read as "no change".

## The op

```
invoke("unscramble", {"sourceId": <n>, "args": {"body": "<base64 image bytes>", "headers": [["x-enc-seed", "..."], ...]}})
```

- Output `{"body": "<base64>"}`: show these bytes. Grid images come back re-encoded as JPEG (quality 90); XOR-only
  images come back decoded.
- Output `{"body": null}`: the bytes are already right; show them as fetched.
- Err `"Failed to decode image"`: a grid image that does not decode. Treat it as a failed page (Kotlin answers 500).
- Err `"unknown op: unscramble"` from a plug-in: same as `{"body": null}`.

Implemented in `sources/comix/src/descramble.rs` (`unscramble`, a pure function) and dispatched in `sources/comix/src/lib.rs`.

## What the app must change

1. **Keep the response headers.** `fetch` returns `(status, headers, bytes)` instead of the bytes, and `SourcePages::image`
   passes them on.
2. **Call the op after a 2xx fetch**, for every WASM source, with the body (base64) and the headers. Show the returned
   bytes when `body` is not null.
3. **Image request headers for comix:** no `Referer` and no `Origin`. The image hosts refuse them. The default set comes
   from `recipe_sources::image_headers`; drop those two for this source.
4. **Retries (the site is flaky).** The `image_url` of a page can end with `#alt=h1,h2,...`. The fragment never reaches the
   server, so the app reads it (Kotlin `altHostFallback` and `proceedWithRetry`):
   - Alt hosts: retry on each listed host, in order, with the query `r=1`, `r=2`, ..., 2 s between tries.
   - On 502, 503, 522, 523 or 404, retry the same URL with `r=<attempt>` up to 10 times, 1.5 s apart.
   - Then try the path variants `/hi/`, `/fcf/`, `/i5/`, `/si/`, `/i/`, `/sii/`, `/ii/` in place of the first
     `/(i5|s?i+)/` segment.
   - Strip the fragment before sending. The plug-in leaves the fragment in place; the app owns the retry.

No change is needed in the plug-in when the site moves its image hosts, only in the app when the retry rules change.
