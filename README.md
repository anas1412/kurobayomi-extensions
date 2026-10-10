# Kurobayomi extensions

Sources for [Kurobayomi](https://github.com/anas1412/kurobayomi): one folder per source, manga in `manga/`, anime in `anime/`.
Each source is a small text recipe (`recipe.toml`) plus saved page samples (`fixtures/`) that the tests use.

Sources are ported from open-source Aniyomi and Mihon extension repositories (Apache-2.0). Each source's README names the
original and what was changed.

## How to add a source to the app

Kurobayomi ships with no online sources. Today you add one by copying its recipe into the app's scrapers folder:

1. Get this repo: `git clone https://github.com/anas1412/kurobayomi-extensions.git`, or download a single `recipe.toml`
   from the source's folder on GitHub (open the file, then "Raw").
2. Create the scrapers folder if it does not exist: `mkdir -p ~/.local/share/kurobayomi/scrapers`
   (with Flatpak the folder is inside the app's own data folder).
3. Copy the recipe and give it the source name, for example:
   `cp manga/existentialcomics/recipe.toml ~/.local/share/kurobayomi/scrapers/existentialcomics.toml`
4. Restart Kurobayomi. Open **Browse**, then **Sources**: the source is in the list under its language.

Installing by pasting this repository's link into the app (Browse, Extensions, repos) is planned and not built yet.

## Sources

| Source | Kind | Status |
|---|---|---|
| `manga/existentialcomics` | manga | works |
| `manga/gwtb` (Gone with the Blastwave) | manga | recipe done, chapter links fixed in the engine |
| `anime/beatzanime` | anime | browse and episode list work, video playback is not working yet |
| `anime/_parked/kimoitv` | anime | parked: the site's bot check blocks playback |

More sources (WeebCentral, Comix, KickAssAnime) are being packaged as downloadable plug-ins and will be added here.
