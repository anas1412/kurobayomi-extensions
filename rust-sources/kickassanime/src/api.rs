//! The kaa.lt JSON API as pure functions: URL builders, the filter encoding and one parser per response, so tests run
//! over saved responses. Ported from `KickAssAnime.kt`, `KickAssAnimeFilters.kt` and `dto/KickAssAnimeDto.kt`.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chrono::Datelike;
use kuro_source::anime::{AnimeStatus, AnimesPage, SAnime, SEpisode};
use kuro_source::json::decode_source_json;
use kuro_source::{Filter, FilterList, SourceError};
use serde::Deserialize;

/// The primary domain. The Kotlin `SEARCH_BASE_URL`; the other KAA domains only redirect here.
pub const BASE_URL: &str = "https://kaa.lt";
pub const API_URL: &str = "https://kaa.lt/api/show";
pub const HOST: &str = "kaa.lt";

/// Audio languages the site names, with the label the episode list shows (Kotlin `LOCALE`).
pub const LOCALES: [(&str, &str); 5] = [
    ("ja-JP", "Japanese"),
    ("en-US", "English"),
    ("es-ES", "Spanish (España)"),
    ("ko-KR", "Korean"),
    ("zh-CN", "Chinese (Simplified)"),
];

// ================================ DTOs ================================

#[derive(Deserialize)]
struct Poster {
    hq: String,
}

#[derive(Deserialize)]
struct Item {
    title: String,
    title_en: Option<String>,
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
struct Search {
    result: Vec<Item>,
    maxPage: i32,
}

#[derive(Deserialize)]
#[allow(non_snake_case)]
struct Recents {
    hadNext: bool,
    result: Vec<Item>,
}

#[derive(Deserialize)]
struct Info {
    #[serde(default)]
    genres: Vec<String>,
    poster: Poster,
    season: Option<String>,
    slug: String,
    #[serde(default)]
    status: String,
    synopsis: Option<String>,
    title: String,
    title_en: Option<String>,
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

/// One server of an episode: its name (`VidStreaming`, `BirdStream`, ...) and the player page.
#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct Server {
    pub name: String,
    pub src: String,
}

#[derive(Deserialize)]
struct Servers {
    servers: Vec<Server>,
}

// ============================== Listings ==============================

fn anime(item: Item, use_english: bool) -> Result<SAnime, SourceError> {
    let title = pick_title(item.title, item.title_en, use_english)?;
    Ok(SAnime {
        url: format!("/{}", item.slug),
        title,
        thumbnail_url: Some(poster_url(&item.poster.hq)),
        ..SAnime::default()
    })
}

/// The English title when asked for and present, else the romaji one. A blank title is a parse error (Kotlin `!!`).
fn pick_title(title: String, title_en: Option<String>, use_english: bool) -> Result<String, SourceError> {
    let chosen = match title_en {
        Some(en) if use_english && !en.trim().is_empty() => en,
        _ => title,
    };
    if chosen.trim().is_empty() { Err(SourceError::Parse("an anime without a title".into())) } else { Ok(chosen) }
}

pub fn poster_url(hq: &str) -> String {
    format!("{BASE_URL}/image/poster/{hq}.webp")
}

fn page_of(items: Vec<Item>, has_next: bool, use_english: bool) -> Result<AnimesPage, SourceError> {
    let animes = items.into_iter().map(|item| anime(item, use_english)).collect::<Result<_, _>>()?;
    Ok(AnimesPage { animes, has_next_page: has_next })
}

pub fn popular_url(page: i32) -> String {
    format!("{API_URL}/trending?page={page}")
}

pub fn latest_url(page: i32) -> String {
    format!("{API_URL}/recent?type=all&page={page}")
}

/// `/api/show/trending` (and the other `show/*` sub-pages): more pages while `page_count > page`.
pub fn parse_popular(body: &[u8], page: i32, use_english: bool) -> Result<AnimesPage, SourceError> {
    let data: Popular = decode_source_json(body)?;
    page_of(data.result, data.page_count > page, use_english)
}

/// `/api/show/recent`: the site says whether a next page exists.
pub fn parse_latest(body: &[u8], use_english: bool) -> Result<AnimesPage, SourceError> {
    let data: Recents = decode_source_json(body)?;
    page_of(data.result, data.hadNext, use_english)
}

/// `/api/fsearch` and `/api/anime`: more pages while `page < maxPage`.
pub fn parse_search(body: &[u8], page: i32, use_english: bool) -> Result<AnimesPage, SourceError> {
    let data: Search = decode_source_json(body)?;
    page_of(data.result, page < data.maxPage, use_english)
}

// =============================== Search ===============================

/// What a search sends: a sub-page that overrides everything, a filtered browse, or a text search.
#[derive(Debug, Clone, PartialEq)]
pub enum SearchRequest {
    /// `GET /api/{sub_page}?page=N`, parsed by its path (see [`parse_sub_page`]).
    SubPage(String),
    /// `GET /api/anime?page=N[&filters=...]`.
    Browse { url: String },
    /// `POST /api/fsearch` with this JSON body.
    Text { body: String },
}

/// The request for a search (Kotlin `searchAnimeRequest`). `filters` is the base64 filter JSON or empty.
pub fn search_request(page: i32, query: &str, params: &SearchParams) -> SearchRequest {
    if !params.sub_page.is_empty() {
        return SearchRequest::SubPage(format!("{BASE_URL}/api/{}?page={page}", params.sub_page));
    }
    let encoded = params.encoded_filters();
    if query.trim().is_empty() {
        let mut url = format!("{BASE_URL}/api/anime?page={page}");
        if !encoded.is_empty() {
            url.push_str("&filters=");
            url.push_str(&encoded);
        }
        SearchRequest::Browse { url }
    } else {
        let mut body = serde_json::json!({ "page": page, "query": query });
        if !encoded.is_empty() {
            body["filters"] = serde_json::Value::String(encoded);
        }
        SearchRequest::Text { body: body.to_string() }
    }
}

/// A sub-page response, parsed by the shape its path implies (Kotlin `searchAnimeParse`).
pub fn parse_sub_page(path: &str, body: &[u8], page: i32, use_english: bool) -> Result<AnimesPage, SourceError> {
    if path.ends_with("/anime") || path.ends_with("api/fsearch") {
        parse_search(body, page, use_english)
    } else if path.ends_with("/recent") {
        parse_latest(body, use_english)
    } else {
        parse_popular(body, page, use_english)
    }
}

/// A search text that names one show: a `slug:` prefix or a link on the primary domain. `Err` for a link elsewhere.
pub fn slug_of_query(query: &str) -> Result<Option<String>, SourceError> {
    if let Some(slug) = query.strip_prefix("slug:") {
        return Ok(Some(slug.to_owned()));
    }
    if !query.starts_with("https://") {
        return Ok(None);
    }
    let url = url::Url::parse(query).map_err(|e| SourceError::Url(e.to_string()))?;
    if url.host_str() != Some(HOST) {
        return Err(SourceError::Url("Unsupported url".into()));
    }
    match url.path_segments().and_then(|mut s| s.next()).filter(|s| !s.is_empty()) {
        Some(slug) => Ok(Some(slug.to_owned())),
        None => Err(SourceError::Url("Unsupported url".into())),
    }
}

// =============================== Filters ==============================

pub const GENRES: [&str; 77] = [
    "Action",
    "Adult Cast",
    "Adventure",
    "Anthropomorphic",
    "Avant Garde",
    "Award Winning",
    "Boys Love",
    "CGDCT",
    "Childcare",
    "Combat Sports",
    "Comedy",
    "Crossdressing",
    "Delinquents",
    "Detective",
    "Drama",
    "Ecchi",
    "Educational",
    "Erotica",
    "Fantasy",
    "Gag Humor",
    "Girls Love",
    "Gore",
    "Gourmet",
    "Harem",
    "High Stakes Game",
    "Historical",
    "Horror",
    "Idols (Female)",
    "Idols (Male)",
    "Isekai",
    "Iyashikei",
    "Josei",
    "Kids",
    "Love Polygon",
    "Magical Sex Shift",
    "Mahou Shoujo",
    "Martial Arts",
    "Mecha",
    "Medical",
    "Military",
    "Music",
    "Mystery",
    "Mythology",
    "Organized Crime",
    "Otaku Culture",
    "Parody",
    "Performing Arts",
    "Pets",
    "Psychological",
    "Racing",
    "Reincarnation",
    "Reverse Harem",
    "Romance",
    "Romantic Subtext",
    "Samurai",
    "School",
    "Sci-Fi",
    "Seinen",
    "Shoujo",
    "Shounen",
    "Showbiz",
    "Slice of Life",
    "Space",
    "Sports",
    "Strategy Game",
    "Super Power",
    "Supernatural",
    "Survival",
    "Suspense",
    "Team Sports",
    "Time Travel",
    "Urban Fantasy",
    "Vampire",
    "Video Game",
    "Villainess",
    "Visual Arts",
    "Workplace",
];
const STATUS: [(&str, &str); 3] = [("All", ""), ("Finished Airing", "finished"), ("Currently Airing", "airing")];
const TYPES: [(&str, &str); 7] = [
    ("All", ""),
    ("TV", "tv"),
    ("Movie", "movie"),
    ("ONA", "ona"),
    ("OVA", "ova"),
    ("SPECIAL", "special"),
    ("TV_SPECIAL", "tv_special"),
];
const SUB_PAGES: [(&str, &str); 5] = [
    ("<Select>", ""),
    ("Trending", "show/trending"),
    ("Anime", "anime"),
    ("Recently Added", "show/recent"),
    ("Popular Shows", "show/popular"),
];

fn names(pairs: &[(&str, &str)]) -> Vec<String> {
    pairs.iter().map(|(name, _)| (*name).to_owned()).collect()
}

/// "All", then this year down to 1967.
fn years() -> Vec<String> {
    let now = chrono::Local::now().year();
    std::iter::once("All".to_owned()).chain((1967..=now).rev().map(|y| y.to_string())).collect()
}

/// The browse filters (Kotlin `FILTER_LIST`).
pub fn filter_list() -> FilterList {
    FilterList(vec![
        Filter::group("Genre", GENRES.iter().map(|g| Filter::check_box(*g)).collect()),
        Filter::select("Year", years()),
        Filter::select("Status", names(&STATUS)),
        Filter::select("Type", names(&TYPES)),
        Filter::separator(""),
        Filter::header("NOTE: Overrides & ignores search and other filters"),
        Filter::select("Sub-page", names(&SUB_PAGES)),
    ])
}

/// What the filters ask for (Kotlin `FilterSearchParams`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SearchParams {
    pub genres: Vec<String>,
    pub year: Option<String>,
    pub status: String,
    pub kind: String,
    pub sub_page: String,
}

impl SearchParams {
    /// The filter object, keys in the Kotlin order (genres, year, status, type), `{}` when nothing is chosen.
    pub fn filters_json(&self) -> String {
        let quoted = |s: &str| serde_json::Value::String(s.to_owned()).to_string();
        let mut parts = Vec::new();
        if !self.genres.is_empty() {
            parts.push(format!("\"genres\":[{}]", self.genres.iter().map(|g| quoted(g)).collect::<Vec<_>>().join(",")));
        }
        if let Some(year) = &self.year {
            parts.push(format!("\"year\":{year}"));
        }
        if !self.status.is_empty() {
            parts.push(format!("\"status\":{}", quoted(&self.status)));
        }
        if !self.kind.is_empty() {
            parts.push(format!("\"type\":{}", quoted(&self.kind)));
        }
        format!("{{{}}}", parts.join(","))
    }

    /// The `filters` parameter: the filter object in base64, empty when nothing is chosen.
    pub fn encoded_filters(&self) -> String {
        let json = self.filters_json();
        if json == "{}" { String::new() } else { STANDARD.encode(json) }
    }
}

fn lookup(pairs: &[(&str, &str)], shown: Option<&str>) -> String {
    pairs.iter().find(|(name, _)| Some(*name) == shown).map(|(_, v)| (*v).to_owned()).unwrap_or_default()
}

/// Reads the filter states (Kotlin `getSearchParameters`). Filters it does not know are ignored.
pub fn search_params(filters: &FilterList) -> SearchParams {
    let mut params = SearchParams::default();
    for filter in filters.iter() {
        match (filter.name(), filter) {
            ("Genre", Filter::Group { state, .. }) => {
                params.genres = state
                    .iter()
                    .filter_map(|f| match f {
                        Filter::CheckBox { name, state: true } => Some(name.clone()),
                        _ => None,
                    })
                    .collect();
            }
            ("Year", _) => params.year = filter.select_value().filter(|v| *v != "All").map(str::to_owned),
            ("Status", _) => params.status = lookup(&STATUS, filter.select_value()),
            ("Type", _) => params.kind = lookup(&TYPES, filter.select_value()),
            ("Sub-page", _) => params.sub_page = lookup(&SUB_PAGES, filter.select_value()),
            _ => {}
        }
    }
    params
}

// =============================== Details ==============================

pub fn details_url(anime_url: &str) -> String {
    format!("{API_URL}{anime_url}")
}

pub fn language_url(anime_url: &str) -> String {
    format!("{API_URL}{anime_url}/language")
}

pub fn parse_languages(body: &[u8]) -> Result<Vec<String>, SourceError> {
    Ok(decode_source_json::<Languages>(body)?.result)
}

/// The label of a language code, or the code itself when the site uses one the Kotlin list does not name.
pub fn locale_name(code: &str) -> String {
    LOCALES.iter().find(|(c, _)| *c == code).map(|(_, name)| (*name).to_owned()).unwrap_or_else(|| code.to_owned())
}

fn status(text: &str) -> AnimeStatus {
    match text {
        "finished_airing" => AnimeStatus::Completed,
        "currently_airing" => AnimeStatus::Ongoing,
        _ => AnimeStatus::Unknown,
    }
}

/// `/api/show/{slug}` plus its `/language` list (Kotlin `animeDetailsParse`).
pub fn parse_details(body: &[u8], languages: &[String], use_english: bool) -> Result<SAnime, SourceError> {
    let info: Info = decode_source_json(body)?;
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
    Ok(SAnime {
        url: format!("/{}", info.slug),
        title: pick_title(info.title, info.title_en, use_english)?,
        thumbnail_url: Some(poster_url(&info.poster.hq)),
        genre: Some(info.genres.join(", ")),
        status: status(&info.status),
        description: Some(description),
        initialized: true,
        ..SAnime::default()
    })
}

// ============================== Episodes ==============================

pub fn episodes_url(anime_url: &str, page: usize, lang: &str) -> String {
    format!("{API_URL}{anime_url}/episodes?page={page}&lang={lang}")
}

/// The languages in the order they are tried: the preferred one, then the second, then the site's order.
pub fn language_order(mut languages: Vec<String>, first: &str, second: &str) -> Vec<String> {
    languages.sort_by_key(|l| (l != first, l != second));
    languages
}

pub fn parse_episode_page(body: &[u8]) -> Result<EpisodePage, SourceError> {
    decode_source_json(body)
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
    pub fn episodes(&self, anime_url: &str, lang: &str) -> Vec<SEpisode> {
        self.result
            .iter()
            .map(|ep| SEpisode {
                name: match ep.title.as_deref().filter(|t| !t.trim().is_empty()) {
                    Some(title) => format!("Ep. {} - {title}", ep.episode_string),
                    None => format!("Ep. {}", ep.episode_string),
                },
                url: format!("{anime_url}/ep-{}-{}", ep.episode_string, ep.slug),
                episode_number: ep.episode_string.parse().unwrap_or(-1.0),
                scanlator: Some(locale_name(lang)),
                ..SEpisode::default()
            })
            .collect()
    }
}

// =============================== Servers ==============================

/// `/api/show/{slug}/ep-1-abc` becomes `/api/show/{slug}/episode/ep-1-abc`.
pub fn servers_url(episode_url: &str) -> String {
    format!("{API_URL}{}", episode_url.replace("/ep-", "/episode/ep-"))
}

pub fn parse_servers(body: &[u8]) -> Result<Vec<Server>, SourceError> {
    Ok(decode_source_json::<Servers>(body)?.servers)
}
