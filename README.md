# Kurobayomi extensions

Sources for [Kurobayomi](https://github.com/anas1412/kurobayomi): one folder per source, manga in `manga/`, anime in `anime/`.
Each source is a small text recipe (`recipe.toml`) plus saved page samples (`fixtures/`) that the tests use.

Sources are ported from open-source Aniyomi and Mihon extension repositories (Apache-2.0). Each source's README names the
original and what was changed.

## How to add this repository to the app

Like Aniyomi, you add a repository by its link; Kurobayomi ships with no online sources.

1. In Kurobayomi open **Browse**, then the menu, then **Extension repos**, and add this link:
   `https://raw.githubusercontent.com/anas1412/kurobayomi-extensions/main/index.min.json`
2. Open **Browse**, then **Extensions**: the sources of this repository are listed. Install the ones you want.
3. They appear under **Browse**, then **Sources**.

**Not working yet:** the repository index file and the install step in the app are still being built, so this link does
not work today. Until then, developers can test a recipe by copying it to `~/.local/share/kurobayomi/scrapers/<name>.toml`
and restarting the app.

## Sources

| Source | Kind | Status |
|---|---|---|
| `manga/existentialcomics` | manga | works |
| `manga/gwtb` (Gone with the Blastwave) | manga | recipe done, chapter links fixed in the engine |
| `anime/beatzanime` | anime | browse and episode list work, video playback is not working yet |
| `anime/_parked/kimoitv` | anime | parked: the site's bot check blocks playback |

More sources (WeebCentral, Comix, KickAssAnime) are being packaged as downloadable plug-ins and will be added here.
