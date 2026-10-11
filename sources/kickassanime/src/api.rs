//! The kaa.lt JSON API as pure functions: URLs, the search request and one parser per response. Ported from
//! `KickAssAnime.kt` and `dto/KickAssAnimeDto.kt` (Apache-2.0, yuzono). Filters and the English-title setting are not
//! here: the app sends no filters to anime packages, and the settings are fixed at their Kotlin defaults.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use url::Url;

pub type Res<T> = Result<T, String>;

pub const BASE_URL: &str = "https://kaa.lt";
pub const API_URL: &str = "https://kaa.lt/api/show";
pub const FSEARCH_URL: &str = "https://kaa.lt/api/fsearch";
pub const HOST: &str = "kaa.lt";

/// Audio languages the site names, with the label the episode list shows (Kotlin `LOCALE`).
const LOCALES: [(&str, &str); 5] = [
    ("ja-JP", "Japanese"),
    ("en-US", "English"),
    ("es-ES", "Spanish (España)"),
    ("ko-KR", "Korean"),
    ("zh-CN", "Chinese (Simplified)"),
];

// ============================ Output (wire) ============================

/// An anime as the app reads it. `description` is set only by a details read.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Anime {
    pub url: String,
    pub title: String,
    pub description: Option<String>,
    pub thumbnail_url: Option<String>,
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
    pub scanlator: Option<String>,
}

/// A server of an episode. Its videos are read later, with the `videos` op, so `videoList` is absent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hoster {
    pub hoster_url: String,
    pub hoster_name: String,
}

// ============================ Input (args) =============================

#[derive(Deserialize)]
pub struct AnimeRef {
    pub url: String,
}

#[derive(Deserialize)]
pub struct EpisodeRef {
    pub url: String,
}

// ================================ DTOs =================================

#[derive(Deserialize)]
struct Poster {
    hq: String,
}

#[derive(Deserialize)]
struct Item {
    title: String,
    slug: String,
    poster: Poster,
}

#[derive(Deserialize)]
struct Popular {
    page_count: i32,
    result: Vec<Item>,
}

#[derive(Deserialize)]
#[allow(non_snake_case)]
struct Recents {
    hadNext: bool,
    result: Vec<Item>,
}

#[derive(Deserialize)]
#[allow(non_snake_case)]
struct Search {
    result: Vec<Item>,
    maxPage: i32,
}

#[derive(Deserialize)]
struct Info {
    slug: String,
    title: String,
    poster: Poster,
    synopsis: Option<String>,
    season: Option<String>,
    year: Option<i32>,
}

#[derive(Deserialize)]
struct Languages {
    result: Vec<String>,
}

#[derive(Deserialize)]
pub struct EpisodePage {
    /// Only the count matters: one entry per page of episodes.
    pages: Vec<serde_json::Value>,
    #[serde(default)]
    result: Vec<EpisodeDto>,
}

#[derive(Deserialize)]
struct EpisodeDto {
    slug: String,
    title: Option<String>,
    episode_string: String,
}

/// One server of an episode: its name (`VidStreaming`, `BirdStream`, ...) and its player page.
#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct Server {
    pub name: String,
    pub src: String,
}

#[derive(Deserialize)]
struct Servers {
    servers: Vec<Server>,
}

pub fn decode<T: DeserializeOwned>(body: &str) -> Res<T> {
    serde_json::from_str(body).map_err(|e| format!("invalid JSON: {e}"))
}

// ============================== Listings ===============================

pub fn poster_url(hq: &str) -> String {
    format!("{BASE_URL}/image/poster/{hq}.webp")
}

fn anime(item: Item) -> Res<Anime> {
    if item.title.trim().is_empty() {
        return Err("an anime without a title".into());
    }
    Ok(Anime {
        url: format!("/{}", item.slug),
        title: item.title,
        description: None,
        thumbnail_url: Some(poster_url(&item.poster.hq)),
    })
}

fn page_of(items: Vec<Item>, has_next_page: bool) -> Res<AnimesPage> {
    let animes = items.into_iter().map(anime).collect::<Res<_>>()?;
    Ok(AnimesPage { animes, has_next_page })
}

pub fn popular_url(page: i32) -> String {
    format!("{API_URL}/trending?page={page}")
}

pub fn latest_url(page: i32) -> String {
    format!("{API_URL}/recent?type=all&page={page}")
}

/// Browse of every show, used by a search with no text.
pub fn browse_url(page: i32) -> String {
    format!("{BASE_URL}/api/anime?page={page}")
}

/// More pages while `page_count > page`.
pub fn parse_popular(body: &str, page: i32) -> Res<AnimesPage> {
    let data: Popular = decode(body)?;
    page_of(data.result, data.page_count > page)
}

/// The site says whether a next page exists.
pub fn parse_latest(body: &str) -> Res<AnimesPage> {
    let data: Recents = decode(body)?;
    page_of(data.result, data.hadNext)
}

/// More pages while `page < maxPage`.
pub fn parse_search(body: &str, page: i32) -> Res<AnimesPage> {
    let data: Search = decode(body)?;
    page_of(data.result, page < data.maxPage)
}

// ================================ Search ===============================

/// The JSON body of a text search (`POST /api/fsearch`).
pub fn search_body(page: i32, query: &str) -> String {
    serde_json::json!({ "page": page, "query": query }).to_string()
}

/// The Referer a search sends (Kotlin `searchAnimeRequest`).
pub fn search_referer(query: &str) -> String {
    let q = url::form_urlencoded::Serializer::new(String::new()).append_pair("q", query).finish();
    format!("{BASE_URL}/search?{q}")
}

/// A `slug:` prefix or a link on kaa.lt names one show. `Err` for a link elsewhere.
pub fn slug_of_query(query: &str) -> Res<Option<String>> {
    if let Some(slug) = query.strip_prefix("slug:") {
        return Ok(Some(slug.to_owned()));
    }
    if !query.starts_with("https://") {
        return Ok(None);
    }
    let url = Url::parse(query).map_err(|e| e.to_string())?;
    if url.host_str() != Some(HOST) {
        return Err("Unsupported url".into());
    }
    match url.path_segments().and_then(|mut s| s.next()).filter(|s| !s.is_empty()) {
        Some(slug) => Ok(Some(slug.to_owned())),
        None => Err("Unsupported url".into()),
    }
}

// ================================ Details ==============================

pub fn details_url(anime_url: &str) -> String {
    format!("{API_URL}{anime_url}")
}

pub fn language_url(anime_url: &str) -> String {
    format!("{API_URL}{anime_url}/language")
}

pub fn parse_languages(body: &str) -> Res<Vec<String>> {
    Ok(decode::<Languages>(body)?.result)
}

/// The label of a language code, or the code itself when the site uses one the Kotlin list does not name.
pub fn locale_name(code: &str) -> String {
    LOCALES.iter().find(|(c, _)| *c == code).map(|(_, name)| (*name).to_owned()).unwrap_or_else(|| code.to_owned())
}

/// `/api/show/{slug}` with its `/language` list: the synopsis, then the dub languages, season and year.
pub fn parse_details(body: &str, languages: &[String]) -> Res<Anime> {
    let info: Info = decode(body)?;
    if info.title.trim().is_empty() {
        return Err("an anime without a title".into());
    }
    let mut description = String::new();
    if let Some(synopsis) = &info.synopsis {
        description.push_str(synopsis);
        description.push_str("\n\n");
    }
    let dubs: Vec<String> = languages.iter().map(|l| locale_name(l)).collect();
    description.push_str(&format!("Available Dub Languages: {}\n", dubs.join(", ")));
    if let Some(season) = &info.season {
        let mut chars = season.chars();
        let capital: String = chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default();
        description.push_str(&format!("Season: {capital}\n"));
    }
    if let Some(year) = info.year {
        description.push_str(&format!("Year: {year}"));
    }
    Ok(Anime {
        url: format!("/{}", info.slug),
        title: info.title,
        description: Some(description),
        thumbnail_url: Some(poster_url(&info.poster.hq)),
    })
}

// ================================ Episodes =============================

pub fn episodes_url(anime_url: &str, page: usize, lang: &str) -> String {
    format!("{API_URL}{anime_url}/episodes?page={page}&lang={lang}")
}

/// The languages in the order they are tried: the preferred one, then the second, then the site's order.
pub fn language_order(mut languages: Vec<String>, first: &str, second: &str) -> Vec<String> {
    languages.sort_by_key(|l| (l != first, l != second));
    languages
}

impl EpisodePage {
    pub fn is_empty(&self) -> bool {
        self.result.is_empty()
    }

    /// How many pages the list has; page 1 is this one.
    pub fn page_count(&self) -> usize {
        self.pages.len().max(1)
    }

    /// The episodes of this page, in site order (oldest first).
    pub fn episodes(&self, anime_url: &str, lang: &str) -> Vec<Episode> {
        self.result
            .iter()
            .map(|ep| Episode {
                name: match ep.title.as_deref().filter(|t| !t.trim().is_empty()) {
                    Some(title) => format!("Ep. {} - {title}", ep.episode_string),
                    None => format!("Ep. {}", ep.episode_string),
                },
                url: format!("{anime_url}/ep-{}-{}", ep.episode_string, ep.slug),
                episode_number: ep.episode_string.parse().unwrap_or(-1.0),
                scanlator: Some(locale_name(lang)),
            })
            .collect()
    }
}

pub fn parse_episode_page(body: &str) -> Res<EpisodePage> {
    decode(body)
}

// ================================ Servers ==============================

/// `/api/show/{slug}/ep-1-abc` becomes `/api/show/{slug}/episode/ep-1-abc`.
pub fn servers_url(episode_url: &str) -> String {
    format!("{API_URL}{}", episode_url.replace("/ep-", "/episode/ep-"))
}

pub fn parse_servers(body: &str) -> Res<Vec<Server>> {
    Ok(decode::<Servers>(body)?.servers)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> &'static str {
        match name {
            "trending.json" => include_str!("../fixtures/trending.json"),
            "recent.json" => include_str!("../fixtures/recent.json"),
            "search.json" => include_str!("../fixtures/search.json"),
            "show.json" => include_str!("../fixtures/show.json"),
            "language.json" => include_str!("../fixtures/language.json"),
            "episodes.json" => include_str!("../fixtures/episodes.json"),
            "servers.json" => include_str!("../fixtures/servers.json"),
            other => panic!("no fixture {other}"),
        }
    }

    #[test]
    fn popular_reads_the_trending_page_and_its_page_count() {
        let page = parse_popular(fixture("trending.json"), 1).unwrap();
        assert_eq!(page.animes.len(), 24);
        assert!(page.has_next_page, "page_count is 28");
        assert_eq!(page.animes[0].title, "Bungou Stray Dogs 5th Season");
        assert_eq!(page.animes[0].url, "/bungo-stray-dogs-5-af88");
        assert_eq!(
            page.animes[0].thumbnail_url.as_deref(),
            Some("https://kaa.lt/image/poster/bungo-stray-dogs-5-3e66-hq.webp")
        );
        assert!(!parse_popular(fixture("trending.json"), 28).unwrap().has_next_page);
        assert_eq!(popular_url(2), "https://kaa.lt/api/show/trending?page=2");
    }

    #[test]
    fn latest_reads_the_site_flag_for_a_next_page() {
        let page = parse_latest(fixture("recent.json")).unwrap();
        assert_eq!(page.animes.len(), 36);
        assert!(page.has_next_page);
        assert_eq!(page.animes[0].url, "/ranma-12-2024-season-3-3a25");
    }

    #[test]
    fn a_text_search_posts_json_and_reads_max_page() {
        assert_eq!(search_body(1, "frieren"), r#"{"page":1,"query":"frieren"}"#);
        let page = parse_search(fixture("search.json"), 1).unwrap();
        assert!(!page.has_next_page, "maxPage is 1");
        assert_eq!(search_referer("a b"), "https://kaa.lt/search?q=a+b");
    }

    #[test]
    fn a_slug_or_a_site_link_names_one_show() {
        assert_eq!(slug_of_query("slug:abc-1").unwrap().as_deref(), Some("abc-1"));
        assert_eq!(slug_of_query("https://kaa.lt/abc-1/x").unwrap().as_deref(), Some("abc-1"));
        assert!(slug_of_query("https://other.test/x").is_err());
        assert_eq!(slug_of_query("frieren").unwrap(), None);
    }

    #[test]
    fn details_build_the_description_from_the_languages() {
        let languages = parse_languages(fixture("language.json")).unwrap();
        assert_eq!(languages, ["ja-JP", "en-US", "es-419", "es-ES"]);
        let anime = parse_details(fixture("show.json"), &languages).unwrap();
        assert_eq!(anime.title, "Sousou no Frieren");
        assert_eq!(anime.url, "/sousou-no-frieren-2d15");
        let description = anime.description.unwrap();
        assert!(description.contains("Available Dub Languages: Japanese, English, es-419, Spanish (España)\n"));
        assert!(description.contains("Season: Fall\n"));
        assert!(description.ends_with("Year: 2023"));
    }

    #[test]
    fn episodes_are_read_newest_first_by_the_caller_and_named_by_their_number() {
        let page = parse_episode_page(fixture("episodes.json")).unwrap();
        assert_eq!(page.page_count(), 1);
        let episodes = page.episodes("/sousou-no-frieren-2d15", "ja-JP");
        assert_eq!(episodes.len(), 28);
        assert_eq!(episodes[0].name, "Ep. 1 - The Journey's End");
        assert_eq!(episodes[0].url, "/sousou-no-frieren-2d15/ep-1-f897b3");
        assert_eq!(episodes[0].episode_number, 1.0);
        assert_eq!(episodes[0].scanlator.as_deref(), Some("Japanese"));
        assert_eq!(language_order(vec!["es-ES".into(), "en-US".into(), "ja-JP".into()], "ja-JP", "en-US"), [
            "ja-JP", "en-US", "es-ES"
        ]);
    }

    #[test]
    fn servers_are_read_from_the_episode_url() {
        assert_eq!(
            servers_url("/s/ep-1-abc"),
            "https://kaa.lt/api/show/s/episode/ep-1-abc"
        );
        let servers = parse_servers(fixture("servers.json")).unwrap();
        assert_eq!(servers.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["VidStreaming", "BirdStream"]);
        assert!(servers[0].src.starts_with("https://krussdomi.com/cat-player/player?id=67d0c079169c31976b8d7970"));
    }
}
