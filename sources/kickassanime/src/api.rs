//! The kaa.lt JSON API as pure functions: URLs, the search request and one parser per response. Ported from
//! `KickAssAnime.kt`, `KickAssAnimeFilters.kt` and `dto/KickAssAnimeDto.kt` (Apache-2.0, yuzono). The English-title
//! setting is fixed at its Kotlin default (off).

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
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
    /// Genres joined with ", " (Kotlin `genre`). Set by a details read only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    /// SAnime status: 0 unknown, 1 ongoing, 2 completed. Set by a details read only.
    #[serde(skip_serializing_if = "is_zero")]
    pub status: i32,
    /// The studio. The show endpoint has none, so it stays empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
}

fn is_zero(n: &i32) -> bool {
    *n == 0
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
    /// Epoch milliseconds of the episode's `created_at`; 0 when the API gives none (never "now").
    pub date_upload: i64,
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
    #[serde(default)]
    genres: Vec<String>,
    #[serde(default)]
    status: Option<String>,
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
    /// ISO-8601 UTC, as the recent list's `created_at`. The episode list of the sampled show has no date field, so
    /// this is usually absent.
    #[serde(default)]
    created_at: Option<String>,
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
        genre: None,
        status: 0,
        author: None,
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

/// Browse of every show, used by a search with no text: `/api/anime?page=N`, with `&filters=` when filters are set.
pub fn browse_url(page: i32, filters: Option<&str>) -> String {
    match filters {
        Some(encoded) => format!("{BASE_URL}/api/anime?page={page}&filters={encoded}"),
        None => format!("{BASE_URL}/api/anime?page={page}"),
    }
}

/// `/api/{sub-page}?page=N`: the sub-page filter overrides the search and the other filters (Kotlin `SubPageFilter`).
pub fn subpage_url(subpage: &str, page: i32) -> String {
    format!("{BASE_URL}/api/{subpage}?page={page}")
}

/// The answer of a sub-page, read as the endpoint's shape (Kotlin `searchAnimeParse`): `anime` is a search page,
/// `show/recent` the latest list, anything else (trending, popular) the popular list.
pub fn parse_subpage(subpage: &str, body: &str, page: i32) -> Res<AnimesPage> {
    match subpage {
        "anime" => parse_search(body, page),
        "show/recent" => parse_latest(body),
        _ => parse_popular(body, page),
    }
}

// ============================ Filters (ABI 1.5) ============================

/// The sub-page choices (Kotlin `SUBPAGE`) by index: "<Select>" is none.
const SUBPAGES: [&str; 5] = ["", "show/trending", "anime", "show/recent", "show/popular"];
/// The status choices (Kotlin `STATUS`) by index, as the JSON text of the `status` value; "" is all.
const STATUSES: [&str; 3] = ["", "\"finished\"", "\"airing\""];
/// The type choices (Kotlin `TYPE`) by index, as the JSON text of the `type` value; "" is all.
const TYPES: [&str; 7] = ["", "\"tv\"", "\"movie\"", "\"ona\"", "\"ova\"", "\"special\"", "\"tv_special\""];

/// The filter called `name` in the app's FilterList (ABI 1.5), by its body; `None` when the app sent no such filter.
fn filter<'a>(filters: &'a [Value], name: &str) -> Option<&'a Value> {
    filters
        .iter()
        .filter_map(|f| f.as_object()?.values().next())
        .find(|body| body.get("name").and_then(Value::as_str) == Some(name))
}

/// The option a Select picked (its index), `None` when the filter is absent.
fn picked_index(filters: &[Value], name: &str) -> Option<usize> {
    usize::try_from(filter(filters, name)?.get("state")?.as_i64()?).ok()
}

/// The display value a Select picked (`values[state]`), `None` when absent.
fn picked_value(filters: &[Value], name: &str) -> Option<String> {
    let index = picked_index(filters, name)?;
    filter(filters, name)?.get("values")?.as_array()?.get(index)?.as_str().map(str::to_owned)
}

/// The sub-page the app picked, `None` for "<Select>" or no filter.
pub fn subpage(filters: &[Value]) -> Option<&'static str> {
    SUBPAGES.get(picked_index(filters, "Sub-page")?).copied().filter(|s| !s.is_empty())
}

/// The JSON text of the filters that are set (Kotlin `getSearchParameters`): `{"genres":[...],"year":N,"status":"x",
/// "type":"y"}`, the keys in that order. `{}` when none is set.
pub fn filters_json(filters: &[Value]) -> String {
    let mut parts: Vec<String> = Vec::new();
    let genres: Vec<String> = filter(filters, "Genre")
        .and_then(|body| body.get("state")?.as_array())
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let body = item.as_object()?.values().next()?;
            if body.get("state")?.as_bool()? {
                serde_json::to_string(body.get("name")?).ok()
            } else {
                None
            }
        })
        .collect();
    if !genres.is_empty() {
        parts.push(format!("\"genres\":[{}]", genres.join(",")));
    }
    // The year is a number in the Kotlin, so only digits are written; "All" is no year.
    if let Some(year) = picked_value(filters, "Year").filter(|y| !y.is_empty() && y.bytes().all(|b| b.is_ascii_digit())) {
        parts.push(format!("\"year\":{year}"));
    }
    if let Some(status) = STATUSES.get(picked_index(filters, "Status").unwrap_or(0)).filter(|s| !s.is_empty()) {
        parts.push(format!("\"status\":{status}"));
    }
    if let Some(kind) = TYPES.get(picked_index(filters, "Type").unwrap_or(0)).filter(|s| !s.is_empty()) {
        parts.push(format!("\"type\":{kind}"));
    }
    format!("{{{}}}", parts.join(","))
}

/// The filters as the site wants them: standard base64 of [`filters_json`], or `None` when no filter is set (the Kotlin
/// sends no `filters` then).
pub fn encoded_filters(filters: &[Value]) -> Option<String> {
    let json = filters_json(filters);
    (json != "{}").then(|| base64(json.as_bytes()))
}

/// Standard base64 with padding (Kotlin `Base64.encodeToString(bytes, NO_WRAP)`).
pub fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = u32::from(chunk[0]) << 16
            | u32::from(*chunk.get(1).unwrap_or(&0)) << 8
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
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

/// The JSON body of a text search (`POST /api/fsearch`), with the encoded filters when there are any.
pub fn search_body(page: i32, query: &str, filters: Option<&str>) -> String {
    let mut body = serde_json::json!({ "page": page, "query": query });
    if let Some(encoded) = filters {
        body["filters"] = Value::String(encoded.to_owned());
    }
    body.to_string()
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

/// The label of a language code (Kotlin `getLocale`): "" for a code the Kotlin list does not name.
pub fn locale_name(code: &str) -> String {
    LOCALES.iter().find(|(c, _)| *c == code).map(|(_, name)| (*name).to_owned()).unwrap_or_default()
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
        genre: Some(info.genres.join(", ")).filter(|g| !g.is_empty()),
        status: parse_status(info.status.as_deref()),
        author: None,
    })
}

/// Kotlin `parseStatus`: `finished_airing` is completed (2), `currently_airing` ongoing (1), else unknown (0).
fn parse_status(status: Option<&str>) -> i32 {
    match status {
        Some("finished_airing") => 2,
        Some("currently_airing") => 1,
        _ => 0,
    }
}

/// Epoch milliseconds of an ISO-8601 UTC timestamp (`2026-10-10T18:15:22.947Z`); 0 when it does not parse.
pub fn iso_millis(text: &str) -> i64 {
    fn num(s: &str) -> Option<i64> {
        s.parse().ok()
    }
    let parse = || -> Option<i64> {
        let (date, time) = text.trim().split_once('T')?;
        let mut d = date.splitn(3, '-');
        let (y, m, day) = (num(d.next()?)?, num(d.next()?)?, num(d.next()?)?);
        let time = time.trim_end_matches('Z');
        let (hms, frac) = time.split_once('.').unwrap_or((time, ""));
        let mut t = hms.splitn(3, ':');
        let (h, mi, s) = (num(t.next()?)?, num(t.next()?)?, num(t.next().unwrap_or("0"))?);
        let ms = format!("{frac:0<3}").get(..3)?.parse::<i64>().ok()?;
        // Days since 1970-01-01 (Hinnant's civil-from-days, inverted).
        let y = if m <= 2 { y - 1 } else { y };
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day - 1;
        let days = era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468;
        Some(((days * 24 + h) * 60 + mi) * 60_000 + s * 1000 + ms).filter(|v| *v > 0)
    };
    parse().unwrap_or(0)
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
                date_upload: ep.created_at.as_deref().map(iso_millis).unwrap_or(0),
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
        assert_eq!(search_body(1, "frieren", None), r#"{"page":1,"query":"frieren"}"#);
        assert_eq!(
            search_body(2, "frieren", Some("e30=")),
            r#"{"filters":"e30=","page":2,"query":"frieren"}"#
        );
        let page = parse_search(fixture("search.json"), 1).unwrap();
        assert!(!page.has_next_page, "maxPage is 1");
        assert_eq!(search_referer("a b"), "https://kaa.lt/search?q=a+b");
    }

    /// The app's filters in the serde form, and the Kotlin text and base64 they make (`getSearchParameters`).
    #[test]
    fn the_filters_are_the_kotlin_json_text_and_its_base64() {
        let filters: Vec<Value> = serde_json::from_str(
            r#"[
                {"Group": {"name": "Genre", "state": [
                    {"CheckBox": {"name": "Action", "state": true}},
                    {"CheckBox": {"name": "Comedy", "state": false}},
                    {"CheckBox": {"name": "Sci-Fi", "state": true}}]}},
                {"Select": {"name": "Year", "values": ["All", "2024", "2023"], "state": 1}},
                {"Select": {"name": "Status", "values": ["All", "Finished Airing", "Currently Airing"], "state": 2}},
                {"Select": {"name": "Type", "values": ["All", "TV"], "state": 1}},
                {"Select": {"name": "Sub-page", "values": ["<Select>", "Trending", "Anime"], "state": 0}}
            ]"#,
        )
        .unwrap();
        let text = r#"{"genres":["Action","Sci-Fi"],"year":2024,"status":"airing","type":"tv"}"#;
        assert_eq!(filters_json(&filters), text);
        assert_eq!(encoded_filters(&filters), Some(base64(text.as_bytes())));
        assert_eq!(subpage(&filters), None, "<Select> is no sub-page");
        assert_eq!(filters_json(&[]), "{}");
        assert_eq!(encoded_filters(&[]), None, "no filter, no filters parameter");
        assert_eq!(browse_url(1, Some("e30=")), "https://kaa.lt/api/anime?page=1&filters=e30=");
    }

    #[test]
    fn a_sub_page_overrides_the_search_and_is_read_as_its_endpoint() {
        let picked: Vec<Value> = serde_json::from_str(
            r#"[{"Select": {"name": "Sub-page", "values": ["<Select>", "Trending", "Anime"], "state": 2}}]"#,
        )
        .unwrap();
        assert_eq!(subpage(&picked), Some("anime"));
        assert_eq!(subpage_url("anime", 2), "https://kaa.lt/api/anime?page=2");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
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
        assert!(
            description.contains("Available Dub Languages: Japanese, English, , Spanish (España)\n"),
            "a code the Kotlin list does not name has the empty label, as in the Kotlin"
        );
        assert!(description.contains("Season: Fall\n"));
        assert!(description.ends_with("Year: 2023"));
        assert!(description.starts_with("During their decade-long quest"));
        assert_eq!(anime.genre.as_deref(), Some("Adventure, Drama, Fantasy, Shounen"));
        assert_eq!(anime.status, 2, "finished_airing is completed");
        assert_eq!(anime.author, None, "the show endpoint has no studio");
        assert_eq!(parse_status(Some("currently_airing")), 1);
        assert_eq!(parse_status(Some("upcoming")), 0);
        assert_eq!(parse_status(None), 0);
    }

    #[test]
    fn iso_dates_become_epoch_milliseconds_and_unknown_is_zero() {
        assert_eq!(iso_millis("1970-01-01T00:00:00.000Z"), 0);
        assert_eq!(iso_millis("2026-10-10T18:15:22.947Z"), 1_791_656_122_947);
        assert_eq!(iso_millis("2023-09-29T00:00:00Z"), 1_695_945_600_000);
        assert_eq!(iso_millis("2024-02-29T12:00:00.5Z"), 1_709_208_000_500);
        assert_eq!(iso_millis(""), 0);
        assert_eq!(iso_millis("not a date"), 0);
    }

    #[test]
    fn an_episode_without_a_date_is_zero_and_one_with_created_at_is_dated() {
        let page = parse_episode_page(fixture("episodes.json")).unwrap();
        assert!(page.episodes("/s", "ja-JP").iter().all(|e| e.date_upload == 0), "no date field: 0, never now");
        let dated = parse_episode_page(
            r#"{"pages":[],"result":[{"slug":"a","title":null,"episode_string":"1","created_at":"2023-09-29T00:00:00.000Z"}]}"#,
        )
        .unwrap();
        assert_eq!(dated.episodes("/s", "ja-JP")[0].date_upload, 1_695_945_600_000);
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
