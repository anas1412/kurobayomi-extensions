//! Kagane's API (`https://kagane.si/api/v2`): the addresses, the request bodies and the parsers. No network here; `lib.rs`
//! sends the requests through the host.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub type Res<T> = Result<T, String>;

pub const BASE: &str = "https://kagane.si";
pub const API: &str = "https://kagane.si/api/v2";
/// Titles a page of the list or search, as the Kotlin asks for.
const PAGE_SIZE: u32 = 35;

// `SManga` status codes.
const ONGOING: i32 = 1;
const COMPLETED: i32 = 2;
const CANCELLED: i32 = 5;
const ON_HIATUS: i32 = 6;

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
    /// -1 lets the app read the number from the name, as the Kotlin leaves it for most series.
    pub chapter_number: f32,
    pub scanlator: String,
}

fn parse<'a, T: Deserialize<'a>>(body: &'a str, what: &str) -> Res<T> {
    serde_json::from_str(body).map_err(|e| format!("unexpected Kagane {what}: {e}"))
}

fn image_url(id: &str) -> String {
    format!("{API}/image/{id}")
}

fn non_empty(text: String) -> Option<String> {
    let text = text.trim().to_owned();
    (!text.is_empty()).then_some(text)
}

// ------------------------------------------------------------------ list and search

/// The search address: page numbers start at 0 on the site. `sort` is empty for relevance.
pub fn search_url(page: i32, sort: &str) -> String {
    let mut url = format!("{API}/search/series?page={}&size={PAGE_SIZE}", (page - 1).max(0));
    if !sort.is_empty() {
        url.push_str("&sort=");
        url.push_str(sort);
        url.push_str(",desc");
    }
    url
}

/// The search body: every source type and English titles, as the Kotlin defaults (all content ratings).
pub fn search_body(query: &str) -> String {
    let mut body = json!({ "source_type": ["Official", "Unofficial", "Mixed"], "content_lang": ["en"] });
    if !query.trim().is_empty() {
        body["title"] = json!(query.trim());
    }
    body.to_string()
}

#[derive(Deserialize)]
struct SearchDto {
    #[serde(default)]
    content: Vec<SeriesDto>,
    #[serde(default = "yes")]
    last: bool,
}

fn yes() -> bool {
    true
}

#[derive(Deserialize)]
struct SeriesDto {
    series_id: String,
    title: String,
    cover_image_id: Option<String>,
}

pub fn parse_search(body: &str) -> Res<MangasPage> {
    let dto: SearchDto = parse(body, "search answer")?;
    let mangas = dto
        .content
        .into_iter()
        .map(|series| Manga {
            url: series.series_id,
            title: series.title.trim().to_owned(),
            artist: None,
            author: None,
            description: None,
            genre: None,
            status: 0,
            thumbnail_url: series.cover_image_id.as_deref().map(image_url),
        })
        .collect();
    Ok(MangasPage { mangas, has_next_page: !dto.last })
}

// ------------------------------------------------------------------ details

pub fn details_url(series: &str) -> String {
    format!("{API}/series/{series}")
}

#[derive(Deserialize)]
struct DetailsDto {
    title: String,
    description: Option<String>,
    upload_status: Option<String>,
    format: Option<String>,
    #[serde(default)]
    series_staff: Vec<StaffDto>,
    #[serde(default)]
    genres: Vec<GenreDto>,
    #[serde(default)]
    series_alternate_titles: Vec<AltTitleDto>,
    #[serde(default)]
    series_links: Vec<LinkDto>,
    average_rating: Option<f64>,
    total_ratings: Option<i64>,
    #[serde(default)]
    series_covers: Vec<CoverDto>,
}

#[derive(Deserialize)]
struct StaffDto {
    name: String,
    role: String,
}

#[derive(Deserialize)]
struct GenreDto {
    genre_name: String,
}

#[derive(Deserialize)]
struct AltTitleDto {
    title: String,
}

#[derive(Deserialize)]
struct LinkDto {
    url: String,
    label: Option<String>,
}

#[derive(Deserialize)]
struct CoverDto {
    image_id: String,
}

/// The status of `upload_status`, case-insensitively (`DetailsDto.toStatus`).
fn status_of(text: &str) -> i32 {
    match text.to_uppercase().as_str() {
        "ONGOING" => ONGOING,
        "COMPLETED" => COMPLETED,
        "HIATUS" => ON_HIATUS,
        "ABANDONED" => CANCELLED,
        _ => 0,
    }
}

/// The text of a description that may carry HTML: tags dropped, the common entities read back.
fn plain(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&amp;", "&")
}

/// The names of the staff whose role contains one of `roles`, once each, in order.
fn staff(staff: &[StaffDto], roles: &[&str]) -> Option<String> {
    let mut names: Vec<&str> = Vec::new();
    for member in staff {
        let role = member.role.to_lowercase();
        if roles.iter().any(|r| role.contains(r)) && !names.contains(&member.name.as_str()) {
            names.push(&member.name);
        }
    }
    non_empty(names.join(", "))
}

/// The details as the Kotlin builds them: rating, summary, links and other titles in the description; the format first
/// among the genres; authors from the "Author" and "Story" roles, artists from "Artist" and "Art".
pub fn parse_details(series: &str, body: &str) -> Res<Manga> {
    let dto: DetailsDto = parse(body, "series details")?;
    let mut description = String::new();
    if let Some(rating) = dto.average_rating {
        description.push_str(&format!("Rating: {:.1}/10", rating / 10.0));
        if let Some(total) = dto.total_ratings {
            description.push_str(&format!(" ({total} ratings)"));
        }
        description.push_str("\n\n");
    }
    if let Some(summary) = dto.description.as_deref().map(plain).and_then(non_empty) {
        description.push_str(&summary);
        description.push('\n');
    }
    if !dto.series_links.is_empty() {
        description.push_str("\nLink(s):\n");
        for link in &dto.series_links {
            description.push_str(&format!("- {}: {}\n", link.label.as_deref().unwrap_or("Link"), link.url));
        }
    }
    if !dto.series_alternate_titles.is_empty() {
        description.push_str("\nAssociated Name(s):\n");
        for alt in &dto.series_alternate_titles {
            description.push_str(&format!("• {}\n", alt.title));
        }
    }
    let genre = dto.format.iter().filter(|f| !f.trim().is_empty()).cloned().chain(dto.genres.into_iter().map(|g| g.genre_name)).collect::<Vec<_>>().join(", ");
    Ok(Manga {
        url: series.to_owned(),
        title: dto.title.trim().to_owned(),
        author: staff(&dto.series_staff, &["author", "story"]),
        artist: staff(&dto.series_staff, &["artist", "art"]),
        description: non_empty(description),
        genre: non_empty(genre),
        status: dto.upload_status.as_deref().map_or(0, status_of),
        thumbnail_url: dto.series_covers.first().map(|cover| image_url(&cover.image_id)),
    })
}

// ------------------------------------------------------------------ chapters

pub fn books_url(series: &str) -> String {
    format!("{API}/series/{series}/books/index?fields=List")
}

/// The compact chapter index: `fields` names the columns of each `books` row.
#[derive(Deserialize)]
struct BookIndexDto {
    fields: Vec<String>,
    #[serde(default)]
    groups: Vec<GroupDto>,
    books: Vec<Vec<Value>>,
}

#[derive(Deserialize)]
struct GroupDto {
    title: String,
}

/// The chapter name in the Kotlin's default ("optional") mode: the title, else "Ch.N", else "Vol.N".
fn chapter_name(title: &str, chapter: Option<&str>, volume: Option<&str>) -> String {
    let title = title.trim();
    if !title.is_empty() {
        return title.to_owned();
    }
    match (chapter.filter(|c| !c.is_empty()), volume.filter(|v| !v.is_empty())) {
        (Some(chapter), _) => format!("Ch.{chapter}"),
        (None, Some(volume)) => format!("Vol.{volume}"),
        (None, None) => String::new(),
    }
}

/// Every chapter, the highest sort number first. The chapter url is the site's reader path, whose last part `pages`
/// reads back as the book id.
pub fn parse_books(series: &str, body: &str) -> Res<Vec<Chapter>> {
    let dto: BookIndexDto = parse(body, "chapter index")?;
    let column = |name: &str| dto.fields.iter().position(|f| f == name);
    let (Some(id), Some(sort)) = (column("book_id"), column("sort_no")) else {
        return Err("the Kagane chapter index has no book_id or sort_no column".into());
    };
    let (title, chapter_no, volume_no, released, groups) = (column("title"), column("chapter_no"), column("volume_no"), column("released_at"), column("group_ix"));
    let cell = |row: &[Value], at: Option<usize>| at.and_then(|at| row.get(at)).cloned().unwrap_or(Value::Null);
    let mut rows: Vec<(f64, Chapter)> = Vec::with_capacity(dto.books.len());
    for row in &dto.books {
        let Some(book) = cell(row, Some(id)).as_str().map(str::to_owned) else { continue };
        let scanlator = cell(row, groups)
            .as_array()
            .map(|ix| ix.iter().filter_map(|i| dto.groups.get(i.as_u64()? as usize)).map(|g| g.title.as_str()).collect::<Vec<_>>().join(", "))
            .unwrap_or_default();
        let chapter = Chapter {
            url: format!("/series/{series}/reader/{book}"),
            name: chapter_name(cell(row, title).as_str().unwrap_or(""), cell(row, chapter_no).as_str(), cell(row, volume_no).as_str()),
            date_upload: cell(row, released).as_i64().map_or(0, |seconds| seconds * 1000),
            chapter_number: -1.0,
            scanlator,
        };
        rows.push((cell(row, Some(sort)).as_f64().unwrap_or(0.0), chapter));
    }
    rows.sort_by(|a, b| b.0.total_cmp(&a.0));
    Ok(rows.into_iter().map(|(_, chapter)| chapter).collect())
}

// ------------------------------------------------------------------ pages

/// The book id of a chapter url: its last path segment.
pub fn book_of(chapter_url: &str) -> Res<&str> {
    chapter_url.trim_end_matches('/').rsplit('/').next().filter(|id| !id.is_empty()).ok_or_else(|| format!("not a Kagane chapter: {chapter_url}"))
}

pub const INTEGRITY_URL: &str = "https://kagane.si/api/integrity";

/// The integrity token and when it runs out (unix seconds).
#[derive(Deserialize, Serialize, Debug, PartialEq)]
pub struct Integrity {
    pub token: String,
    pub exp: u64,
}

pub fn parse_integrity(body: &str) -> Res<Integrity> {
    parse(body, "integrity answer")
}

pub fn challenge_url(book: &str) -> String {
    format!("{API}/books/{book}?is_datasaver=false")
}

#[derive(Deserialize)]
struct ChallengeDto {
    access_token: String,
    cache_url: String,
    manifest: Option<ManifestDto>,
}

#[derive(Deserialize)]
struct ManifestDto {
    #[serde(default)]
    pages: Vec<PageDto>,
}

#[derive(Deserialize)]
struct PageDto {
    page_no: i32,
    page_id: String,
    ext: Option<String>,
}

/// The page images of a book, in page order: each on the cache host, with the access token the challenge gave.
pub fn parse_pages(book: &str, body: &str) -> Res<Vec<String>> {
    let dto: ChallengeDto = parse(body, "page list")?;
    let mut pages = dto.manifest.map(|m| m.pages).unwrap_or_default();
    pages.sort_by_key(|page| page.page_no);
    let cache = dto.cache_url.trim_end_matches('/');
    Ok(pages
        .iter()
        .map(|page| format!("{cache}/api/v2/books/page/{book}/{}.{}?token={}", page.page_id, page.ext.as_deref().unwrap_or("jxl"), dto.access_token))
        .collect())
}

/// The error text of a refused request, when the API gives one.
pub fn error_message(body: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body).ok()?;
    ["message", "detail", "error"].iter().find_map(|key| value.get(*key)?.as_str().map(str::to_owned))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH: &str = include_str!("../fixtures/search.json");
    const DETAILS: &str = include_str!("../fixtures/details.json");
    const BOOKS: &str = include_str!("../fixtures/books.json");
    const CHALLENGE: &str = include_str!("../fixtures/challenge.json");

    #[test]
    fn search_urls_count_pages_from_zero_and_sort_descending() {
        assert_eq!(search_url(1, "total_views"), "https://kagane.si/api/v2/search/series?page=0&size=35&sort=total_views,desc");
        assert_eq!(search_url(3, ""), "https://kagane.si/api/v2/search/series?page=2&size=35");
    }

    #[test]
    fn the_search_body_names_the_title_only_when_there_is_one() {
        let body: Value = serde_json::from_str(&search_body(" one piece ")).unwrap();
        assert_eq!(body["title"], "one piece");
        assert_eq!(body["content_lang"], json!(["en"]));
        let empty: Value = serde_json::from_str(&search_body("  ")).unwrap();
        assert!(empty.get("title").is_none());
    }

    #[test]
    fn a_search_page_gives_the_series_ids_titles_and_covers() {
        let page = parse_search(SEARCH).unwrap();
        assert_eq!(page.mangas.len(), 3);
        let first = &page.mangas[0];
        assert_eq!(first.title, "One Piece");
        assert_eq!(first.url, "019c3a10-837e-7d7c-bf6c-006df658a021");
        assert!(first.thumbnail_url.as_deref().unwrap().starts_with("https://kagane.si/api/v2/image/"));
    }

    #[test]
    fn details_read_the_status_genres_cover_and_summary() {
        let manga = parse_details("s1", DETAILS).unwrap();
        assert_eq!((manga.url.as_str(), manga.title.as_str(), manga.status), ("s1", "One Piece", ONGOING));
        assert!(manga.genre.as_deref().unwrap().starts_with("Manga, Drama"));
        assert!(manga.description.as_deref().unwrap().contains("Monkey D. Luffy"));
        assert_eq!(manga.thumbnail_url.as_deref(), Some("https://kagane.si/api/v2/image/019e2c21-50db-789e-95a9-c20199434cf3"));
    }

    #[test]
    fn statuses_and_html_follow_the_kotlin() {
        assert_eq!((status_of("Completed"), status_of("HIATUS"), status_of("Abandoned"), status_of("?")), (COMPLETED, ON_HIATUS, CANCELLED, 0));
        assert_eq!(plain("<p>A &amp; B</p><br>C"), "A & BC");
    }

    #[test]
    fn chapters_come_newest_first_with_reader_urls_dates_and_groups() {
        let chapters = parse_books("s1", BOOKS).unwrap();
        assert_eq!(chapters.len(), 5);
        let oldest = chapters.last().unwrap();
        assert_eq!(oldest.url, "/series/s1/reader/019e2d3b-e841-705c-a47c-c8e4659731d2");
        assert_eq!(oldest.date_upload, 1_778_875_361_000);
        assert_eq!(oldest.scanlator, "The Hours Between");
        assert!(oldest.name.starts_with("Chapter 1"));
        assert_eq!(book_of(&oldest.url).unwrap(), "019e2d3b-e841-705c-a47c-c8e4659731d2");
    }

    #[test]
    fn an_untitled_chapter_is_named_by_its_number_or_volume() {
        assert_eq!(chapter_name(" ", Some("12"), Some("2")), "Ch.12");
        assert_eq!(chapter_name("", None, Some("2")), "Vol.2");
        assert_eq!(chapter_name("Prologue", Some("0"), None), "Prologue");
    }

    #[test]
    fn pages_point_at_the_cache_host_with_the_access_token() {
        let pages = parse_pages("b1", CHALLENGE).unwrap();
        assert_eq!(pages.len(), 3);
        assert_eq!(pages[0], "https://kstatic.to/api/v2/books/page/b1/f2edc4fd-4921-50c1-a2e7-d6d7ba814dca.jxl?token=ACCESS");
    }

    #[test]
    fn the_integrity_answer_and_errors_are_read() {
        assert_eq!(parse_integrity(r#"{"token":"t","exp":5}"#).unwrap(), Integrity { token: "t".into(), exp: 5 });
        assert_eq!(error_message(r#"{"message":"slow down"}"#).as_deref(), Some("slow down"));
    }
}
