//! The AnimeGG site (<https://www.animegg.org>) as pure functions: the addresses and one parser per page. Ported from
//! `AnimeGG.kt` (Kohi-den, Apache-2.0); the page selectors are the Kotlin's, checked against the live site.

use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};

pub type Res<T> = Result<T, String>;

pub const BASE: &str = "https://www.animegg.org";

// SAnime status codes.
const ONGOING: i32 = 1;
const COMPLETED: i32 = 2;

// ============================ Output (wire) ============================

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Anime {
    pub url: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub thumbnail_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    pub status: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimesPage {
    pub animes: Vec<Anime>,
    pub has_next_page: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Episode {
    pub url: String,
    pub name: String,
    pub episode_number: f32,
    pub scanlator: String,
}

/// One player of an episode: its embed page, read later by the `videos` op.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hoster {
    pub hoster_url: String,
    pub hoster_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Video {
    pub video_url: String,
    pub video_title: String,
    pub resolution: Option<i32>,
    pub headers: Vec<(String, String)>,
    pub preferred: bool,
}

// ================================ Urls =================================

pub fn popular_url() -> String {
    format!("{BASE}/popular-series?sortBy=hits&sortDirection=DESC&ongoing&limit=50&start=0")
}

pub fn latest_url() -> String {
    format!("{BASE}/popular-series?sortBy=createdAt&sortDirection=DESC&ongoing&limit=50&start=0")
}

pub fn search_url(query: &str) -> String {
    format!("{BASE}/search/?q={}", encode(query.trim()))
}

/// Percent-encodes a query value (`application/x-www-form-urlencoded` style).
fn encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// The site address of a stored path (`/series/naruto`, `/naruto-episode-1`, `/embed/25881`).
pub fn absolute(path: &str) -> String {
    if path.starts_with("http") { path.to_owned() } else { format!("{BASE}{path}") }
}

/// The path of a site link, as the Kotlin's `setUrlWithoutDomain` keeps it.
fn path_of(href: &str) -> String {
    href.strip_prefix(BASE).unwrap_or(href).to_owned()
}

/// The headers a video request needs: the site as referer (the video host answers 500 without it).
pub fn video_headers() -> Vec<(String, String)> {
    vec![("Referer".to_owned(), BASE.to_owned())]
}

// =============================== Parsers ===============================

fn selector(css: &str) -> Selector {
    Selector::parse(css).expect("a valid selector")
}

fn text_of(element: ElementRef) -> String {
    element.text().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

fn first_text(root: ElementRef, css: &str) -> Option<String> {
    root.select(&selector(css)).next().map(text_of).filter(|t| !t.is_empty())
}

fn first_attr(root: ElementRef, css: &str, attr: &str) -> Option<String> {
    root.select(&selector(css)).next().and_then(|e| e.value().attr(attr)).map(str::to_owned).filter(|v| !v.is_empty())
}

/// The popular and latest lists (`.fea` cards). The site lists them on one page, so there is no next page.
pub fn parse_list(html: &str) -> AnimesPage {
    let doc = Html::parse_document(html);
    let animes = doc
        .select(&selector(".fea"))
        .filter_map(|card| {
            let href = first_attr(card, ".rightpop a", "href")?;
            Some(Anime {
                url: path_of(&href),
                title: first_text(card, ".rightpop a").unwrap_or_default(),
                description: None,
                thumbnail_url: first_attr(card, "img", "src").map(|src| absolute(&src)),
                genre: None,
                status: 0,
            })
        })
        .collect();
    AnimesPage { animes, has_next_page: false }
}

/// The search results (`.mse` links).
pub fn parse_search(html: &str) -> AnimesPage {
    let doc = Html::parse_document(html);
    let animes = doc
        .select(&selector(".mse"))
        .filter_map(|link| {
            let href = link.value().attr("href")?;
            Some(Anime {
                url: path_of(href),
                title: first_text(link, ".first h2").unwrap_or_default(),
                description: None,
                thumbnail_url: first_attr(link, "img", "src").map(|src| absolute(&src)),
                genre: None,
                status: 0,
            })
        })
        .collect();
    AnimesPage { animes, has_next_page: false }
}

/// The status of an "Status: Completed" span.
fn status_of(span: &str) -> i32 {
    let status = span.split_once("Status:").map_or(span, |(_, rest)| rest).trim();
    if status.contains("Completed") {
        COMPLETED
    } else if status.contains("Ongoing") {
        ONGOING
    } else {
        0
    }
}

/// The series page: title, summary, genres, poster and status.
pub fn parse_details(html: &str, url: &str) -> Anime {
    let doc = Html::parse_document(html);
    let root = doc.root_element();
    let status = root.select(&selector(".infoami span")).map(text_of).find(|span| span.contains("Status")).map_or(0, |span| status_of(&span));
    let genres: Vec<String> = root.select(&selector(".tagscat a")).map(text_of).filter(|g| !g.is_empty()).collect();
    Anime {
        url: url.to_owned(),
        title: first_text(root, ".media-body h1").unwrap_or_default(),
        description: first_text(root, ".ptext"),
        thumbnail_url: first_attr(root, ".media .media-object", "src").map(|src| absolute(&src)),
        genre: Some(genres.join(", ")).filter(|g| !g.is_empty()),
        status,
    }
}

/// The number at the end of "Naruto 12" (the first of a range "12-13"), as the Kotlin's `getEpNumber`.
fn episode_number(text: &str) -> Option<f32> {
    let last = text.split_whitespace().last()?;
    last.split('-').next()?.parse().ok()
}

/// "12" for a whole number, "12.5" otherwise (the Kotlin's `formatEp`).
fn format_number(number: f32) -> String {
    if number.fract() == 0.0 { format!("{number:.0}") } else { format!("{number:.1}") }
}

/// The episode list, as the page orders it (newest first). An episode is named by its title when the title already says
/// the number, else "Episode N - Title"; its audio versions (SUBBED, DUBBED, HD...) are the scanlator.
pub fn parse_episodes(html: &str) -> Vec<Episode> {
    let doc = Html::parse_document(html);
    doc.select(&selector(".newmanga li div"))
        .enumerate()
        .filter_map(|(index, row)| {
            let href = first_attr(row, ".anm_det_pop", "href")?;
            let number = first_text(row, ".anm_det_pop strong").and_then(|t| episode_number(&t)).unwrap_or(index as f32 + 1.0);
            let title = first_text(row, ".anititle").unwrap_or_default();
            let shown = format_number(number);
            let name = if title.contains(&shown) { title } else { format!("Episode {shown} - {title}") };
            let scanlator = row.select(&selector(".btn-xs")).map(text_of).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(", ");
            Some(Episode { url: path_of(&href), name, episode_number: number, scanlator })
        })
        .collect()
}

/// The language of a player tab, by its id (`subbed-Animegg`), as the Kotlin names it.
fn mode_of(tab_id: &str) -> &'static str {
    match tab_id {
        "subbed-Animegg" => "[SUBBED]",
        "dubbed-Animegg" => "[DUBBED]",
        "raw-Animegg" => "[RAW]",
        _ => "",
    }
}

/// The players of an episode page: one per tab, subbed first (the Kotlin's default language).
pub fn parse_hosters(html: &str) -> Vec<Hoster> {
    let doc = Html::parse_document(html);
    let mut hosters: Vec<Hoster> = doc
        .select(&selector(".tab-pane"))
        .filter_map(|tab| {
            let src = first_attr(tab, "iframe", "src")?;
            let mode = mode_of(tab.value().attr("id").unwrap_or(""));
            Some(Hoster { hoster_url: absolute(&src), hoster_name: format!("{mode} AnimeGG").trim().to_owned() })
        })
        .collect();
    hosters.sort_by_key(|hoster| !hoster.hoster_name.starts_with("[SUBBED]"));
    hosters
}

/// The value of `key: "..."` in one object of the player's `videoSources` (a JavaScript literal, not JSON).
fn js_field<'a>(object: &'a str, key: &str) -> Option<&'a str> {
    let after = &object[object.find(&format!("{key}:"))? + key.len() + 1..];
    let start = after.find('"')? + 1;
    let end = start + after[start..].find('"')?;
    Some(&after[start..end])
}

/// The videos of an embed page: its `var videoSources = [...]` list, each `file` on the embed's own host, named
/// "[SUBBED] AnimeGG:480p" as the Kotlin does. The best quality first.
pub fn parse_embed(html: &str, embed_url: &str, hoster_name: &str, preferred: bool) -> Vec<Video> {
    let Some(start) = html.find("var videoSources =") else { return Vec::new() };
    let list = &html[start..];
    let list = &list[..list.find("];").map_or(list.len(), |end| end + 1)];
    let origin = embed_url.find("://").and_then(|scheme| embed_url[scheme + 3..].find('/').map(|path| &embed_url[..scheme + 3 + path])).unwrap_or(BASE);
    let mode = hoster_name.strip_suffix(" AnimeGG").unwrap_or("").trim();
    let mut videos: Vec<Video> = list
        .split('{')
        .skip(1)
        .filter_map(|object| {
            let file = js_field(object, "file")?;
            let label = js_field(object, "label").unwrap_or("");
            let url = if file.starts_with("http") { file.to_owned() } else { format!("{origin}{file}") };
            Some(Video {
                video_url: url,
                video_title: format!("{mode} AnimeGG:{label}").trim().to_owned(),
                resolution: label.trim_end_matches('p').parse().ok(),
                headers: vec![("Referer".to_owned(), origin.to_owned())],
                preferred: false,
            })
        })
        .collect();
    videos.sort_by_key(|video| std::cmp::Reverse(video.resolution.unwrap_or(0)));
    if let Some(first) = videos.first_mut() {
        first.preferred = preferred;
    }
    videos
}

#[cfg(test)]
mod tests {
    use super::*;

    const POPULAR: &str = include_str!("../fixtures/popular.html");
    const SEARCH: &str = include_str!("../fixtures/search.html");
    const SERIES: &str = include_str!("../fixtures/series.html");
    const EPISODE: &str = include_str!("../fixtures/episode.html");
    const EMBED: &str = include_str!("../fixtures/embed.html");

    #[test]
    fn the_lists_and_search_give_series_paths_titles_and_posters() {
        let popular = parse_list(POPULAR);
        assert_eq!(popular.animes.len(), 3);
        assert_eq!((popular.animes[0].url.as_str(), popular.animes[0].title.as_str()), ("/series/one-piece", "One Piece"));
        assert!(popular.animes[0].thumbnail_url.as_deref().unwrap().starts_with("https://vidcache.net"));
        let search = parse_search(SEARCH);
        assert_eq!(search.animes.len(), 3);
        assert_eq!((search.animes[0].url.as_str(), search.animes[0].title.as_str()), ("/series/naruto", "Naruto"));
        assert_eq!(search_url(" naruto shippuden "), "https://www.animegg.org/search/?q=naruto+shippuden");
    }

    #[test]
    fn details_read_the_title_summary_genres_and_status() {
        let anime = parse_details(SERIES, "/series/naruto");
        assert_eq!(anime.title, "Naruto");
        assert_eq!(anime.status, COMPLETED);
        assert_eq!(anime.genre.as_deref(), Some("Action, Martial Arts, Super Power, Comedy, Shounen"));
        assert!(anime.description.as_deref().unwrap().starts_with("Before Naruto was born"));
        assert!(anime.thumbnail_url.is_some());
    }

    #[test]
    fn episodes_are_named_numbered_and_tagged_newest_first() {
        let episodes = parse_episodes(SERIES);
        assert_eq!(episodes.len(), 3);
        let newest = &episodes[0];
        assert_eq!((newest.url.as_str(), newest.episode_number), ("/naruto-episode-220", 220.0));
        assert_eq!(newest.name, "Episode 220 - Going on a Journey");
        assert!(newest.scanlator.contains("SUBBED") && newest.scanlator.contains("DUBBED"));
    }

    #[test]
    fn numbers_and_names_follow_the_kotlin() {
        assert_eq!(episode_number("Naruto 12"), Some(12.0));
        assert_eq!(episode_number("Bleach 12-13"), Some(12.0));
        assert_eq!(episode_number("Movie 7.5"), Some(7.5));
        assert_eq!((format_number(3.0), format_number(3.5)), ("3".to_owned(), "3.5".to_owned()));
    }

    #[test]
    fn hosters_are_the_tabs_subbed_first() {
        let hosters = parse_hosters(EPISODE);
        assert_eq!(hosters.len(), 2);
        assert_eq!(hosters[0], Hoster { hoster_url: "https://www.animegg.org/embed/25881".into(), hoster_name: "[SUBBED] AnimeGG".into() });
        assert_eq!(hosters[1].hoster_name, "[DUBBED] AnimeGG");
    }

    #[test]
    fn the_embed_lists_its_files_on_its_own_host() {
        let videos = parse_embed(EMBED, "https://www.animegg.org/embed/25881", "[SUBBED] AnimeGG", true);
        assert_eq!(videos.len(), 1);
        let video = &videos[0];
        assert!(video.video_url.starts_with("https://www.animegg.org/play/248322/video.mp4?for="), "{}", video.video_url);
        assert_eq!((video.video_title.as_str(), video.resolution, video.preferred), ("[SUBBED] AnimeGG:480p", Some(480), true));
        assert_eq!(video.headers, vec![("Referer".to_owned(), "https://www.animegg.org".to_owned())]);
    }
}
