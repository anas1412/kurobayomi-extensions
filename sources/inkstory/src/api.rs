//! InkStory's API (`https://api.inuko.me/v2`): the addresses, the parsers and the page image key. No network here;
//! `lib.rs` sends the requests through the host.

use serde::{Deserialize, Serialize};

pub type Res<T> = Result<T, String>;

pub const API: &str = "https://api.inuko.me/v2";
/// The `serviceName` the shared API knows this site by (the Kotlin's `serviceName`).
pub const SERVICE: &str = "inkstory";
/// Titles a page, as the Kotlin asks for.
const PAGE_SIZE: u32 = 30;

// `SManga` status codes.
const ONGOING: i32 = 1;
const COMPLETED: i32 = 2;
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
    pub chapter_number: f32,
    pub scanlator: String,
}

fn parse<'a, T: Deserialize<'a>>(body: &'a str, what: &str) -> Res<T> {
    serde_json::from_str(body).map_err(|e| format!("unexpected InkStory {what}: {e}"))
}

fn non_empty(text: Option<&str>) -> Option<String> {
    text.map(str::trim).filter(|t| !t.is_empty()).map(str::to_owned)
}

/// Encodes a query value (the search text).
fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

// ------------------------------------------------------------------ list and search

/// The catalogue, most viewed first (the Kotlin's Popular and search), filtered by `query`. Pages count from 0 on the
/// site.
pub fn books_url(page: i32, query: &str) -> String {
    let mut url = format!("{API}/books?serviceName={SERVICE}");
    if !query.trim().is_empty() {
        url.push_str("&search=");
        url.push_str(&encode(query.trim()));
    }
    url.push_str(&format!("&page={}&size={PAGE_SIZE}&sort=viewsCount,desc", (page - 1).max(0)));
    url
}

/// The chapter update feed, one row a title (the Kotlin's Latest).
pub fn feed_url(page: i32) -> String {
    format!("{API}/chapter-update-feed?serviceName={SERVICE}&onlyBorderChapters=true&page={}&size={PAGE_SIZE}", (page - 1).max(0))
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct NameDto {
    ru: Option<String>,
    en: Option<String>,
    original: Option<String>,
}

impl NameDto {
    /// The Russian title, else the English, else the original, else the slug.
    fn resolve(&self, slug: &str) -> String {
        [&self.ru, &self.en, &self.original].into_iter().find_map(|n| non_empty(n.as_deref())).unwrap_or_else(|| slug.to_owned())
    }
}

#[derive(Deserialize)]
struct BookDto {
    slug: String,
    poster: Option<String>,
    #[serde(default)]
    name: NameDto,
}

impl BookDto {
    fn manga(self) -> Manga {
        Manga {
            title: self.name.resolve(&self.slug),
            url: self.slug,
            artist: None,
            author: None,
            description: None,
            genre: None,
            status: 0,
            thumbnail_url: self.poster,
        }
    }
}

/// A catalogue page. `total` is the `x-estimated-total-hits` header; without it a full page means there may be more.
pub fn parse_books(body: &str, page: i32, total: Option<u64>) -> Res<MangasPage> {
    let books: Vec<BookDto> = parse(body, "book list")?;
    let full = books.len() >= PAGE_SIZE as usize;
    let has_next_page = total.map_or(full, |total| (page.max(1) as u64) * u64::from(PAGE_SIZE) < total);
    Ok(MangasPage { mangas: books.into_iter().map(BookDto::manga).collect(), has_next_page })
}

#[derive(Deserialize)]
struct FeedDto {
    book: BookDto,
}

pub fn parse_feed(body: &str) -> Res<MangasPage> {
    let rows: Vec<FeedDto> = parse(body, "update feed")?;
    let has_next_page = rows.len() >= PAGE_SIZE as usize;
    Ok(MangasPage { mangas: rows.into_iter().map(|row| row.book.manga()).collect(), has_next_page })
}

// ------------------------------------------------------------------ details

/// A title by its slug, the manga url (as in the Kotlin, so Aniyomi backups match).
pub fn details_url(slug: &str) -> String {
    format!("{API}/books/{slug}")
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DetailsDto {
    id: String,
    slug: String,
    #[serde(default)]
    name: NameDto,
    poster: Option<String>,
    description: Option<String>,
    status: Option<String>,
    #[serde(default)]
    labels: Vec<LabelDto>,
    #[serde(default)]
    formats: Vec<String>,
    #[serde(default)]
    relations: Vec<RelationDto>,
    #[serde(default)]
    external_links: Vec<String>,
    average_rating: Option<f64>,
    rating_votes_count: Option<i64>,
    views_count: Option<i64>,
    likes_count: Option<i64>,
    bookmarks_count: Option<i64>,
}

#[derive(Deserialize)]
struct LabelDto {
    name: Option<String>,
}

#[derive(Deserialize)]
struct RelationDto {
    #[serde(rename = "type")]
    kind: Option<String>,
    publisher: Option<PublisherDto>,
}

#[derive(Deserialize)]
struct PublisherDto {
    name: Option<String>,
}

fn status_of(text: &str) -> i32 {
    match text {
        "ONGOING" => ONGOING,
        "DONE" => COMPLETED,
        "FROZEN" => ON_HIATUS,
        _ => 0,
    }
}

/// 1234567 as "1.2M", 1234 as "1.2k" (the Kotlin's `formatInt`).
fn short(value: i64) -> String {
    match value {
        v if v >= 1_000_000 => format!("{:.1}M", v as f64 / 1_000_000.0),
        v if v >= 1_000 => format!("{:.1}k", v as f64 / 1_000.0),
        v => v.to_string(),
    }
}

/// The publishers of the relations of one type, once each, in order.
fn relation(relations: &[RelationDto], kind: &str) -> Option<String> {
    let mut names: Vec<String> = Vec::new();
    for r in relations.iter().filter(|r| r.kind.as_deref() == Some(kind)) {
        if let Some(name) = non_empty(r.publisher.as_ref().and_then(|p| p.name.as_deref()))
            && !names.contains(&name)
        {
            names.push(name);
        }
    }
    (!names.is_empty()).then(|| names.join(", "))
}

/// The book id the chapter and branch lists ask for.
pub fn book_id(body: &str) -> Res<String> {
    Ok(parse::<DetailsDto>(body, "book")?.id)
}

/// The details as the Kotlin builds them: summary, rating, counts and links in the description; labels, then formats,
/// as genres; authors and artists from the relations.
pub fn parse_details(body: &str) -> Res<Manga> {
    let dto: DetailsDto = parse(body, "book")?;
    let mut lines: Vec<String> = Vec::new();
    if let Some(summary) = non_empty(dto.description.as_deref()) {
        lines.push(summary);
        lines.push(String::new());
    }
    if let Some(rating) = dto.average_rating {
        let votes = dto.rating_votes_count.map(|v| format!(" (оценок: {v})")).unwrap_or_default();
        lines.push(format!("Рейтинг: {rating:.2}{votes}"));
    }
    for (label, count) in [("Просмотры", dto.views_count), ("Лайки", dto.likes_count), ("Закладки", dto.bookmarks_count)] {
        if let Some(count) = count {
            lines.push(format!("{label}: {}", short(count)));
        }
    }
    if !dto.external_links.is_empty() {
        lines.push("Внешние ссылки:".to_owned());
        lines.extend(dto.external_links.iter().map(|link| format!("- {link}")));
    }
    let genre: Vec<String> = dto
        .labels
        .iter()
        .filter_map(|l| non_empty(l.name.as_deref()))
        .chain(dto.formats.iter().map(|f| f.to_lowercase().replace('_', " ")))
        .collect();
    Ok(Manga {
        title: dto.name.resolve(&dto.slug),
        author: relation(&dto.relations, "AUTHOR"),
        artist: relation(&dto.relations, "ARTIST"),
        description: non_empty(Some(&lines.join("\n"))),
        genre: (!genre.is_empty()).then(|| genre.join(", ")),
        status: dto.status.as_deref().map_or(0, status_of),
        thumbnail_url: dto.poster,
        url: dto.slug,
    })
}

// ------------------------------------------------------------------ chapters

pub fn branches_url(book_id: &str) -> String {
    format!("{API}/branches?bookId={book_id}&moderationStatus=APPROVED")
}

pub fn chapters_url(book_id: &str) -> String {
    format!("{API}/chapters?bookId={book_id}&moderationStatus=APPROVED")
}

#[derive(Deserialize)]
struct BranchDto {
    id: String,
    #[serde(default)]
    publishers: Vec<PublisherDto>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChapterDto {
    id: String,
    name: Option<String>,
    title: Option<String>,
    number: Option<f64>,
    volume: Option<f64>,
    branch_id: Option<String>,
    created_at: Option<String>,
}

/// 3.0 as "3", 0.5 as "0.5".
fn number(value: f64) -> String {
    let text = value.to_string();
    text.strip_suffix(".0").map(str::to_owned).unwrap_or(text)
}

/// "Том 3 Глава 200 - Конец" as the Kotlin names a chapter.
fn chapter_name(dto: &ChapterDto) -> String {
    let base = match (dto.volume.map(number), dto.number.map(number)) {
        (Some(v), Some(n)) => format!("Том {v} Глава {n}"),
        (None, Some(n)) => format!("Глава {n}"),
        (Some(v), None) => format!("Том {v}"),
        (None, None) => "Глава".to_owned(),
    };
    match [&dto.name, &dto.title].into_iter().find_map(|t| non_empty(t.as_deref())) {
        Some(subtitle) if subtitle.to_lowercase() != base.to_lowercase() => format!("{base} - {subtitle}"),
        _ => base,
    }
}

/// Every approved chapter of every branch (the Kotlin's default "all branches" mode), in the API's order (newest first);
/// a branch's translators are the scanlator. The chapter url is its id, as in the Kotlin.
pub fn parse_chapters(branches: &str, chapters: &str) -> Res<Vec<Chapter>> {
    let branches: Vec<BranchDto> = parse(branches, "branch list")?;
    let teams = |id: &str| {
        let mut names: Vec<String> = Vec::new();
        for p in branches.iter().filter(|b| b.id == id).flat_map(|b| &b.publishers) {
            if let Some(name) = non_empty(p.name.as_deref())
                && !names.contains(&name)
            {
                names.push(name);
            }
        }
        names.join(", ")
    };
    let chapters: Vec<ChapterDto> = parse(chapters, "chapter list")?;
    Ok(chapters
        .iter()
        .map(|dto| Chapter {
            url: dto.id.clone(),
            name: chapter_name(dto),
            date_upload: dto.created_at.as_deref().map_or(0, iso_millis),
            chapter_number: dto.number.map_or(-1.0, |n| n as f32),
            scanlator: dto.branch_id.as_deref().map(teams).unwrap_or_default(),
        })
        .collect())
}

/// Epoch milliseconds of an ISO-8601 UTC timestamp (`2023-05-31T15:20:01.449Z`); 0 when it does not parse.
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

// ------------------------------------------------------------------ pages

pub fn pages_url(chapter_id: &str) -> String {
    format!("{API}/chapters/{chapter_id}")
}

#[derive(Deserialize)]
struct PagesDto {
    #[serde(default)]
    pages: Vec<PageDto>,
}

#[derive(Deserialize)]
struct PageDto {
    index: Option<i32>,
    image: Option<String>,
}

/// The length of an image file name without its extension, and where in it the codec letter sits: `s` for the
/// server-side cipher, `x` for the XOR one.
const IMAGE_NAME_LENGTH: usize = 36;
const IMAGE_MODE_INDEX: usize = 14;

/// An `s` (server cipher) image asked for as its `x` (XOR) twin, which `unxor` reads; any other url as it is.
fn xor_twin(url: &str) -> String {
    let start = url.rfind('/').map_or(0, |i| i + 1);
    let file = url[start..].split('?').next().unwrap_or("");
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    if stem.len() == IMAGE_NAME_LENGTH && stem.as_bytes()[IMAGE_MODE_INDEX] == b's' {
        let at = start + IMAGE_MODE_INDEX;
        return format!("{}x{}", &url[..at], &url[at + 1..]);
    }
    url.to_owned()
}

/// The page images in page order.
pub fn parse_pages(body: &str) -> Res<Vec<String>> {
    let mut pages = parse::<PagesDto>(body, "page list")?.pages;
    pages.sort_by_key(|p| p.index.unwrap_or(i32::MAX));
    Ok(pages.iter().filter_map(|p| non_empty(p.image.as_deref())).map(|url| xor_twin(&url)).collect())
}

/// The key of the XOR cipher on page images (the Kotlin's `SECRET_KEY`).
const KEY: &[u8] = b"UySkp0BzPhwlvP2V";
/// The bytes the image signatures need.
const SIGNATURE: usize = 12;

/// Whether the bytes start like a JPEG, PNG, GIF, WebP or AVIF image.
fn looks_like_image(b: &[u8]) -> bool {
    b.len() >= SIGNATURE
        && (b.starts_with(&[0xFF, 0xD8, 0xFF])
            || b.starts_with(b"\x89PNG")
            || b.starts_with(b"GIF8")
            || (b.starts_with(b"RIFF") && &b[8..12] == b"WEBP")
            || &b[4..11] == b"ftypavi")
}

fn xor(bytes: &mut [u8]) {
    for (i, b) in bytes.iter_mut().enumerate() {
        *b ^= KEY[i % KEY.len()];
    }
}

/// The XOR-ciphered page image read back, or `None` when the bytes already are an image (or are not this cipher).
pub fn unxor(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.len() < SIGNATURE || looks_like_image(bytes) {
        return None;
    }
    let mut head = bytes[..SIGNATURE].to_vec();
    xor(&mut head);
    if !looks_like_image(&head) {
        return None;
    }
    let mut plain = bytes.to_vec();
    xor(&mut plain);
    Some(plain)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOOKS: &str = include_str!("../fixtures/books.json");
    const FEED: &str = include_str!("../fixtures/feed.json");
    const DETAILS: &str = include_str!("../fixtures/details.json");
    const BRANCHES: &str = include_str!("../fixtures/branches.json");
    const CHAPTERS: &str = include_str!("../fixtures/chapters.json");
    const PAGES: &str = include_str!("../fixtures/pages.json");
    const PAGE_START: &[u8] = include_bytes!("../fixtures/page-start.bin");

    #[test]
    fn list_urls_count_pages_from_zero_and_encode_the_query() {
        assert_eq!(books_url(1, ""), "https://api.inuko.me/v2/books?serviceName=inkstory&page=0&size=30&sort=viewsCount,desc");
        assert_eq!(books_url(2, " solo лев "), "https://api.inuko.me/v2/books?serviceName=inkstory&search=solo%20%D0%BB%D0%B5%D0%B2&page=1&size=30&sort=viewsCount,desc");
        assert_eq!(feed_url(3), "https://api.inuko.me/v2/chapter-update-feed?serviceName=inkstory&onlyBorderChapters=true&page=2&size=30");
    }

    #[test]
    fn a_book_list_gives_slugs_russian_titles_and_posters() {
        let page = parse_books(BOOKS, 1, Some(21427)).unwrap();
        assert_eq!(page.mangas.len(), 3);
        assert_eq!((page.mangas[0].url.as_str(), page.mangas[0].title.as_str()), ("solo-leveling", "Поднятие уровня в одиночку"));
        assert!(page.mangas[0].thumbnail_url.as_deref().unwrap().starts_with("https://static.inuko.me/"));
        assert!(page.has_next_page);
        assert!(!parse_books(BOOKS, 1, Some(3)).unwrap().has_next_page, "the total header ends the list");
        assert!(!parse_books(BOOKS, 1, None).unwrap().has_next_page, "a short page without the header ends it too");
    }

    #[test]
    fn the_feed_gives_the_books_of_the_updates() {
        let page = parse_feed(FEED).unwrap();
        assert_eq!(page.mangas[0].url, "a-genius-warrior-who-remembers-everything");
        assert_eq!(page.mangas[0].title, "Гений боевых искусств, помнящий всё");
    }

    #[test]
    fn details_read_the_status_genres_people_and_summary() {
        let manga = parse_details(DETAILS).unwrap();
        assert_eq!((manga.url.as_str(), manga.status), ("solo-leveling", COMPLETED));
        assert!(manga.description.as_deref().unwrap().starts_with("Десятилетие назад"));
        assert!(manga.description.as_deref().unwrap().contains("Рейтинг: 9.63"));
        assert!(manga.genre.as_deref().unwrap().contains("Городское фэнтези"));
        assert!(manga.genre.as_deref().unwrap().ends_with("webtoon, colored"));
        assert!(manga.author.is_some());
        assert_eq!(book_id(DETAILS).unwrap(), "4fe5dc17-e2d0-417c-8224-034a7c6bdb44");
    }

    #[test]
    fn chapters_carry_names_numbers_dates_and_teams() {
        let chapters = parse_chapters(BRANCHES, CHAPTERS).unwrap();
        assert_eq!(chapters.len(), 5);
        let first = &chapters[0];
        assert_eq!(first.url, "37ec6a2a-392d-4075-8e44-ffe0e9a61e53");
        assert_eq!(first.name, "Том 3 Глава 200 - КОНЕЦ");
        assert_eq!((first.chapter_number, first.date_upload), (200.0, 1_685_546_401_449));
        assert_eq!(first.scanlator, "Novate");
        assert_eq!(chapters[3].name, "Том 1 Глава 0.5");
        assert_eq!(chapters[4].name, "Том 1 Глава 0 - Пролог");
    }

    #[test]
    fn pages_come_in_order_and_server_cipher_images_ask_for_the_xor_twin() {
        let pages = parse_pages(PAGES).unwrap();
        assert_eq!(pages.len(), 3);
        assert_eq!(pages[0], "https://static.inuko.me/chapters/37ec6a2a-392d-4075-8e44-ffe0e9a61e53/dff94600-0c3d-xb6d-965c-e22b3a0a06e0.jpeg");
        assert_eq!(xor_twin("https://h/c/dff94600-0c3d-sb6d-965c-e22b3a0a06e0.jpeg?v=1"), "https://h/c/dff94600-0c3d-xb6d-965c-e22b3a0a06e0.jpeg?v=1");
        assert_eq!(xor_twin("https://h/c/poster.jpeg"), "https://h/c/poster.jpeg");
    }

    #[test]
    fn a_xor_page_reads_back_as_a_jpeg_and_a_plain_image_is_left_alone() {
        let plain = unxor(PAGE_START).unwrap();
        assert!(plain.starts_with(&[0xFF, 0xD8, 0xFF]));
        assert_eq!(unxor(&plain), None);
        assert_eq!(unxor(b"not an image at all"), None);
    }

    #[test]
    fn dates_read_with_and_without_fractions() {
        assert_eq!(iso_millis("2018-11-17T06:58:18Z"), 1_542_437_898_000);
        assert_eq!(iso_millis("2024-03-13T16:49:08.530187Z"), 1_710_348_548_530);
        assert_eq!(iso_millis(""), 0);
    }
}
