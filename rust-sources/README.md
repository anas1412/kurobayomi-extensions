# Rust sources (work in progress)

WeebCentral, Comix and KickAssAnime, written in Rust against Kurobayomi's source interfaces (`kuro-source`, `kuro-source-http`,
`kuro-net`). Each folder was a crate in the app's workspace; they were moved here so the app repository ships with no
site-specific code.

Status:

| Source | Kind | State |
|---|---|---|
| `weebcentral` | manga | works against the real site (live check passed) |
| `comix` | manga | works against the real site (live check passed); signing and descrambling ported |
| `kickassanime` | anime | works against the real site; plays HLS with subtitles and audio tracks |

These do **not** build on their own: their `Cargo.toml` files use the app workspace's dependency versions. The plan is to turn
each one into a downloadable plug-in (a WASM component, or a text recipe where the site allows it) that the app fetches when you add
this repository's link. Until then they are the reference implementation and the way to test the app against real sources.

Each source's code is a port of the matching open-source Android extension (Apache-2.0): keiyoushi `weebcentral`, `comix`, and
yuzono `kickassanime`. The licence and credits are kept in the original files; changes are marked by this repository's history.
