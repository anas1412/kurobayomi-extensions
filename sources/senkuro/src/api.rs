//! Senkuro's GraphQL API (`https://api.senkuro.com/graphql`): the queries, the request bodies and the parsers. No
//! network here; `lib.rs` sends the requests through the host.

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

pub type Res<T> = Result<T, String>;

/// The site's two addresses, as the Kotlin offers them: the public one first, then the one for Russia (where the public
/// one may be blocked). The public one is geo-blocked from Russia's side and the Russian one from outside it.
pub const MIRRORS: [&str; 2] = ["https://senkuro.com", "https://senkuro.me"];
/// Titles a page of the list or search; the API's fixed page size.
const PAGE_SIZE: usize = 10;

// `SManga` status codes.
const ONGOING: i32 = 1;
const COMPLETED: i32 = 2;
const CANCELLED: i32 = 5;
const ON_HIATUS: i32 = 6;

/// The GraphQL endpoint of a mirror: `https://senkuro.com` → `https://api.senkuro.com/graphql`.
pub fn api_url(base: &str) -> String {
    format!("{}/graphql", base.replace("https://", "https://api."))
}

const SEARCH_QUERY: &str = "query searchTachiyomiManga($query: String, $rating: MangaTachiyomiSearchRatingFilter, \
$orderBy: MangaTachiyomiOrder, $offset: Int) { mangaTachiyomiSearch(query: $query, rating: $rating, orderBy: $orderBy, \
offset: $offset) { mangas { id slug titles { lang content } cover { original { url } } } } }";

const DETAILS_QUERY: &str = "query fetchTachiyomiManga($mangaId: ID!) { mangaTachiyomiInfo(mangaId: $mangaId) { id slug \
titles { lang content } alternativeNames { lang content } localizations { lang description } type rating status formats \
labels { titles { lang content } } cover { original { url } } mainStaff { roles person { name } } } }";

const CHAPTERS_QUERY: &str = "query fetchTachiyomiChapters($mangaId: ID!) { mangaTachiyomiChapters(mangaId: $mangaId) { \
message chapters { id slug name teamIds number volume createdAt updatedAt } teams { id name } } }";

const PAGES_QUERY: &str = "query fetchTachiyomiChapterPages($mangaId: ID!, $chapterId: ID!) { \
mangaTachiyomiChapterPages(mangaId: $mangaId, chapterId: $chapterId) { pages { url } } }";

fn body(operation: &str, query: &str, variables: Value) -> String {
    json!({ "operationName": operation, "query": query, "variables": variables }).to_string()
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Manga {
    pub url: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    pub status: i32,
    pub thumbnail_url: Option<String>,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MangasPage {
    pub mangas: Vec<Manga>,
    pub has_next_page: bool,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Chapter {
    pub url: String,
    pub name: String,
    pub date_upload: i64,
    pub chapter_number: f32,
    pub scanlator: Option<String>,
}

/// A GraphQL answer: `data`, or the first of `errors` when there is no data.
#[derive(Deserialize)]
struct Answer<T> {
    data: Option<T>,
    #[serde(default)]
    errors: Vec<ErrorDto>,
}

#[derive(Deserialize)]
struct ErrorDto {
    message: String,
}

fn parse<T: DeserializeOwned>(body: &str, what: &str) -> Res<T> {
    let answer: Answer<T> = serde_json::from_str(body).map_err(|e| format!("unexpected Senkuro {what}: {e}"))?;
    match (answer.data, answer.errors.into_iter().next()) {
        (Some(data), _) => Ok(data),
        (None, Some(error)) => Err(format!("Senkuro: {}", error.message)),
        (None, None) => Err(format!("Senkuro sent no {what}")),
    }
}

fn non_empty(text: String) -> Option<String> {
    let text = text.trim().to_owned();
    (!text.is_empty()).then_some(text)
}

#[derive(Deserialize)]
struct TitleDto {
    lang: String,
    content: String,
}

/// The Russian title, else the English one, else the first (`toSManga`).
fn title_of(titles: &[TitleDto]) -> String {
    ["RU", "EN"]
        .iter()
        .find_map(|lang| titles.iter().find(|t| t.lang == *lang))
        .or(titles.first())
        .map(|t| t.content.trim().to_owned())
        .unwrap_or_default()
}

#[derive(Deserialize)]
struct CoverDto {
    original: Option<ImageDto>,
}

#[derive(Deserialize)]
struct ImageDto {
    url: String,
}

fn cover_url(cover: Option<CoverDto>) -> Option<String> {
    cover?.original.map(|image| image.url)
}

/// The manga url, as the Kotlin keeps it so Aniyomi backups match: `{id},,{slug}`.
fn manga_url(id: &str, slug: &str) -> String {
    format!("{id},,{slug}")
}

// ------------------------------------------------------------------ list and search

/// The search body: the most popular first, `query` when there is one, 18+ titles only when the user shows adult titles.
pub fn search_body(page: i32, query: &str, show_adult: bool) -> String {
    let mut variables = json!({
        "orderBy": { "direction": "DESC", "field": "POPULARITY_SCORE" },
        "offset": (page.max(1) as usize - 1) * PAGE_SIZE,
    });
    if !query.trim().is_empty() {
        variables["query"] = json!(query.trim());
    }
    if !show_adult {
        variables["rating"] = json!({ "include": [], "exclude": ["EXPLICIT"] });
    }
    body("searchTachiyomiManga", SEARCH_QUERY, variables)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchData {
    manga_tachiyomi_search: SearchDto,
}

#[derive(Deserialize)]
struct SearchDto {
    mangas: Vec<SearchMangaDto>,
}

#[derive(Deserialize)]
struct SearchMangaDto {
    id: String,
    slug: String,
    #[serde(default)]
    titles: Vec<TitleDto>,
    cover: Option<CoverDto>,
}

pub fn parse_search(body: &str) -> Res<MangasPage> {
    let mangas: Vec<Manga> = parse::<SearchData>(body, "search answer")?
        .manga_tachiyomi_search
        .mangas
        .into_iter()
        .map(|m| Manga {
            url: manga_url(&m.id, &m.slug),
            title: title_of(&m.titles),
            artist: None,
            author: None,
            description: None,
            genre: None,
            status: 0,
            thumbnail_url: cover_url(m.cover),
        })
        .collect();
    Ok(MangasPage { has_next_page: mangas.len() >= PAGE_SIZE, mangas })
}

// ------------------------------------------------------------------ details

/// The GraphQL id of a manga or chapter url: the part before the first `,,`.
pub fn manga_id(url: &str) -> &str {
    url.split(",,").next().unwrap_or(url)
}

pub fn details_body(manga_url: &str) -> String {
    body("fetchTachiyomiManga", DETAILS_QUERY, json!({ "mangaId": manga_id(manga_url) }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DetailsData {
    manga_tachiyomi_info: Option<DetailsDto>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DetailsDto {
    id: String,
    slug: String,
    #[serde(default)]
    titles: Vec<TitleDto>,
    #[serde(default)]
    alternative_names: Vec<TitleDto>,
    #[serde(default)]
    localizations: Vec<LocalizationDto>,
    #[serde(rename = "type")]
    kind: Option<String>,
    rating: Option<String>,
    status: Option<String>,
    #[serde(default)]
    formats: Vec<String>,
    #[serde(default)]
    labels: Vec<LabelDto>,
    cover: Option<CoverDto>,
    #[serde(default)]
    main_staff: Vec<StaffDto>,
}

#[derive(Deserialize)]
struct LocalizationDto {
    lang: String,
    description: Option<String>,
}

#[derive(Deserialize)]
struct LabelDto {
    #[serde(default)]
    titles: Vec<TitleDto>,
}

#[derive(Deserialize)]
struct StaffDto {
    roles: Vec<String>,
    person: PersonDto,
}

#[derive(Deserialize)]
struct PersonDto {
    name: String,
}

fn status_of(status: Option<&str>) -> i32 {
    match status {
        Some("FINISHED") => COMPLETED,
        Some("ONGOING" | "ANNOUNCE") => ONGOING,
        Some("HIATUS") => ON_HIATUS,
        Some("CANCELLED") => CANCELLED,
        _ => 0,
    }
}

fn type_name(slug: &str) -> Option<&'static str> {
    Some(match slug {
        "MANGA" => "Манга",
        "MANHWA" => "Манхва",
        "MANHUA" => "Маньхуа",
        "COMICS" => "Комикс",
        "OEL_MANGA" => "OEL Манга",
        "RU_MANGA" => "РуМанга",
        _ => return None,
    })
}

fn age_name(slug: &str) -> Option<&'static str> {
    Some(match slug {
        "GENERAL" => "0+",
        "SENSITIVE" => "12+",
        "QUESTIONABLE" => "16+",
        "EXPLICIT" => "18+",
        _ => return None,
    })
}

fn format_name(slug: &str) -> Option<&'static str> {
    Some(match slug {
        "DIGEST" => "Сборник",
        "DOUJINSHI" => "Додзинси",
        "IN_COLOR" => "В цвете",
        "SINGLE" => "Сингл",
        "WEB" => "Веб",
        "WEBTOON" => "Вебтун",
        "YONKOMA" => "Ёнкома",
        "SHORT" => "Short",
        _ => return None,
    })
}

/// `text` with its first letter in upper case.
fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map(|first| first.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// The names of the staff with one of `roles`, joined.
fn staff(staff: &[StaffDto], roles: &[&str]) -> Option<String> {
    non_empty(staff.iter().filter(|s| s.roles.iter().any(|r| roles.contains(&r.as_str()))).map(|s| s.person.name.as_str()).collect::<Vec<_>>().join(", "))
}

/// The details as the Kotlin builds them: other titles then the Russian summary in the description; type, age rating,
/// formats and the Russian label names as the genres.
pub fn parse_details(body: &str) -> Res<Manga> {
    let dto = parse::<DetailsData>(body, "manga details")?.manga_tachiyomi_info.ok_or("Senkuro: manga not found")?;
    let mut description = String::new();
    let alt = dto.alternative_names.iter().map(|t| t.content.as_str()).collect::<Vec<_>>().join(" / ");
    if !alt.is_empty() {
        description.push_str(&format!("Альтернативные названия:\n{alt}\n\n"));
    }
    if let Some(summary) = dto.localizations.iter().find(|l| l.lang == "RU").and_then(|l| l.description.as_deref()) {
        description.push_str(summary);
    }
    let labels = dto.labels.iter().filter_map(|l| l.titles.iter().find(|t| t.lang == "RU")).map(|t| t.content.as_str());
    let genre = dto
        .kind
        .as_deref()
        .and_then(type_name)
        .into_iter()
        .chain(dto.rating.as_deref().and_then(age_name))
        .chain(dto.formats.iter().filter_map(|f| format_name(f)))
        .chain(labels)
        .map(|g| capitalized(g.trim()))
        .filter(|g| !g.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    Ok(Manga {
        url: manga_url(&dto.id, &dto.slug),
        title: title_of(&dto.titles),
        author: staff(&dto.main_staff, &["STORY", "STORY_AND_ART", "ORIGINAL_CREATOR"]),
        artist: staff(&dto.main_staff, &["ART", "STORY_AND_ART"]),
        description: non_empty(description),
        genre: non_empty(genre),
        status: status_of(dto.status.as_deref()),
        thumbnail_url: cover_url(dto.cover),
    })
}

// ------------------------------------------------------------------ chapters

pub fn chapters_body(manga_url: &str) -> String {
    body("fetchTachiyomiChapters", CHAPTERS_QUERY, json!({ "mangaId": manga_id(manga_url) }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChaptersData {
    manga_tachiyomi_chapters: ChaptersDto,
}

#[derive(Deserialize)]
struct ChaptersDto {
    #[serde(default)]
    chapters: Vec<ChapterDto>,
    #[serde(default)]
    teams: Vec<TeamDto>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChapterDto {
    id: String,
    slug: String,
    name: Option<String>,
    #[serde(default)]
    team_ids: Vec<String>,
    number: String,
    volume: String,
    created_at: Option<String>,
    updated_at: Option<String>,
}

#[derive(Deserialize)]
struct TeamDto {
    id: String,
    name: String,
}

/// Every chapter in the API's order (newest first). The url is the Kotlin's `{manga url},,{chapter id},,{chapter slug}`,
/// so Aniyomi backups match.
pub fn parse_chapters(manga_url: &str, body: &str) -> Res<Vec<Chapter>> {
    let dto = parse::<ChaptersData>(body, "chapter list")?.manga_tachiyomi_chapters;
    Ok(dto
        .chapters
        .into_iter()
        .map(|c| Chapter {
            url: format!("{manga_url},,{},,{}", c.id, c.slug),
            name: format!("{}. Глава {} {}", c.volume, c.number, c.name.as_deref().unwrap_or("")).trim().to_owned(),
            date_upload: c.updated_at.or(c.created_at).as_deref().map_or(0, iso_millis),
            // -1 lets the app read the number from the name
            chapter_number: c.number.parse().unwrap_or(-1.0),
            scanlator: non_empty(c.team_ids.iter().filter_map(|id| dto.teams.iter().find(|t| &t.id == id)).map(|t| t.name.as_str()).collect::<Vec<_>>().join(", ")),
        })
        .collect())
}

/// Epoch milliseconds of an ISO-8601 UTC timestamp, with or without the `Z` (`2026-10-16T11:11:02.755203`); 0 when it
/// does not parse.
fn iso_millis(text: &str) -> i64 {
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

// ------------------------------------------------------------------ pages

/// The pages body of a chapter url (`{manga id},,{manga slug},,{chapter id},,{chapter slug}`).
pub fn pages_body(chapter_url: &str) -> Res<String> {
    let parts: Vec<&str> = chapter_url.split(",,").collect();
    let [manga, _, chapter, ..] = parts[..] else {
        return Err(format!("not a Senkuro chapter: {chapter_url}"));
    };
    Ok(body("fetchTachiyomiChapterPages", PAGES_QUERY, json!({ "mangaId": manga, "chapterId": chapter })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PagesData {
    manga_tachiyomi_chapter_pages: PagesDto,
}

#[derive(Deserialize)]
struct PagesDto {
    pages: Vec<ImageDto>,
}

pub fn parse_pages(body: &str) -> Res<Vec<String>> {
    Ok(parse::<PagesData>(body, "page list")?.manga_tachiyomi_chapter_pages.pages.into_iter().map(|p| p.url).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const POPULAR: &str = include_str!("../fixtures/popular.json");
    const SEARCH: &str = include_str!("../fixtures/search.json");
    const DETAILS: &str = include_str!("../fixtures/details.json");
    const CHAPTERS: &str = include_str!("../fixtures/chapters.json");
    const PAGES: &str = include_str!("../fixtures/pages.json");

    #[test]
    fn mirrors_map_to_their_api_host() {
        assert_eq!(api_url(MIRRORS[0]), "https://api.senkuro.com/graphql");
        assert_eq!(api_url(MIRRORS[1]), "https://api.senkuro.me/graphql");
    }

    #[test]
    fn the_search_body_pages_by_ten_and_hides_18_plus_unless_asked() {
        let body: Value = serde_json::from_str(&search_body(3, " one piece ", false)).unwrap();
        let vars = &body["variables"];
        assert_eq!((vars["offset"].as_u64(), vars["query"].as_str()), (Some(20), Some("one piece")));
        assert_eq!(vars["rating"]["exclude"], json!(["EXPLICIT"]));
        assert_eq!(vars["orderBy"]["field"], "POPULARITY_SCORE");
        let adult: Value = serde_json::from_str(&search_body(1, "", true)).unwrap();
        assert!(adult["variables"].get("rating").is_none() && adult["variables"].get("query").is_none());
        assert_eq!(adult["variables"]["offset"], 0);
    }

    #[test]
    fn a_full_page_has_a_next_one_with_russian_titles() {
        let page = parse_search(POPULAR).unwrap();
        assert_eq!((page.mangas.len(), page.has_next_page), (10, true));
        let first = &page.mangas[0];
        assert_eq!(first.title, "Скупщик ненужных навыков");
        assert_eq!(first.url, "TUFOR0E6MjUzNTkzNzY5MDI1MjI2MjQ5,,iranai-skill-kaitorimasu");
        assert!(first.thumbnail_url.as_deref().unwrap().starts_with("https://mirai.senkuro.net/"));
        let search = parse_search(SEARCH).unwrap();
        assert!(!search.has_next_page && search.mangas.iter().any(|m| m.url.ends_with(",,one-piece")));
    }

    #[test]
    fn details_read_the_staff_genres_status_and_summary() {
        let manga = parse_details(DETAILS).unwrap();
        assert_eq!((manga.title.as_str(), manga.status), ("Скупщик ненужных навыков", ONGOING));
        assert_eq!((manga.author.as_deref(), manga.artist.as_deref()), (Some("Hiru Kuma"), Some("Keyaki Kobayashi")));
        assert!(manga.genre.as_deref().unwrap().starts_with("Манга, 16+, Веб, Магия"));
        let description = manga.description.unwrap();
        assert!(description.starts_with("Альтернативные названия:\n") && description.contains("В этом мире"));
    }

    #[test]
    fn chapters_keep_the_kotlin_url_and_name_with_teams_and_dates() {
        let chapters = parse_chapters("M,,slug", CHAPTERS).unwrap();
        assert_eq!(chapters.len(), 5);
        let first = &chapters[0];
        assert_eq!(first.url, "M,,slug,,TUFOR0FfQ0hBUFRFUjoyNTM1OTQxMDA0NTkxMjgzMjY,,253594100459128326");
        assert_eq!((first.name.as_str(), first.chapter_number), ("1. Глава 17.2", 17.2));
        assert_eq!(first.scanlator.as_deref(), Some("CVmanga, Типочек"));
        assert_eq!(first.date_upload, 1_792_149_062_755);
    }

    #[test]
    fn pages_read_the_chapter_ids_and_the_image_urls() {
        let body: Value = serde_json::from_str(&pages_body("M,,s,,C,,cs").unwrap()).unwrap();
        assert_eq!((body["variables"]["mangaId"].as_str(), body["variables"]["chapterId"].as_str()), (Some("M"), Some("C")));
        assert!(pages_body("M,,s").is_err());
        let pages = parse_pages(PAGES).unwrap();
        assert_eq!(pages.len(), 4);
        assert!(pages[1].starts_with("https://mirai.senkuro.net/manga-chapters/"));
    }

    #[test]
    fn graphql_errors_and_dates_are_read() {
        assert_eq!(parse_pages(r#"{"data":null,"errors":[{"message":"chapter not found"}]}"#), Err("Senkuro: chapter not found".into()));
        assert_eq!(iso_millis("1970-01-02T00:00:00Z"), 86_400_000);
        assert_eq!(iso_millis("nope"), 0);
    }
}
