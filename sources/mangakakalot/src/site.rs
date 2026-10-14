//! The Mangakakalot site (<https://www.mangakakalot.gg>) as pure functions: URLs and one parser per page. Ported from the
//! keiyoushi `en/mangakakalot` extension and its MangaBox base (Apache-2.0). Without the genre, status and sort filters, the
//! alternative CDNs and the option that merges strip images.

use scraper::node::Node;
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};
use url::Url;

pub type Res<T> = Result<T, String>;

pub const BASE: &str = "https://www.mangakakalot.gg";
pub const HOST: &str = "www.mangakakalot.gg";

// ============================ Output (wire) ============================

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manga {
    pub url: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    /// SManga status: 0 unknown, 1 ongoing, 2 completed.
    pub status: i32,
    pub thumbnail_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MangasPage {
    pub mangas: Vec<Manga>,
    pub has_next_page: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Chapter {
    pub url: String,
    pub name: String,
    pub date_upload: i64,
    pub chapter_number: f32,
    pub scanlator: String,
}

// ================================ URLs =================================

pub fn popular_url(page: i32) -> String {
    format!("{BASE}/manga-list/hot-manga?page={}", page.max(1))
}

pub fn latest_url(page: i32) -> String {
    format!("{BASE}/manga-list/latest-manga?page={}", page.max(1))
}

/// A title search. The site wants the query as a slug (`one_piece`), made as its own script `change_alias` does.
pub fn search_url(page: i32, query: &str) -> String {
    let slug = query_slug(query);
    let encoded: String = url::form_urlencoded::byte_serialize(slug.as_bytes()).collect();
    format!("{BASE}/search/story/{encoded}?page={}", page.max(1))
}

/// Lower case, every punctuation mark and space an underscore, runs of them one, none at the ends.
pub fn query_slug(query: &str) -> String {
    let mut out = String::new();
    for c in query.to_lowercase().chars() {
        let separator = c.is_whitespace() || "!@%^*()+=<>?/,.:;'\"&#[]~-$_".contains(c);
        if separator {
            if !out.ends_with('_') {
                out.push('_');
            }
        } else {
            out.push(c);
        }
    }
    out.trim_matches('_').to_owned()
}

pub fn chapters_url(manga_url: &str) -> Res<String> {
    Ok(format!("{BASE}/api/manga/{}/chapters?limit=-1", slug_of(manga_url)?))
}

/// A page of the site from a stored address (`/manga/...`), refused when it is another site's.
pub fn page(path: &str) -> Res<String> {
    let url = Url::parse(BASE).and_then(|base| base.join(path)).map_err(|e| format!("invalid address {path}: {e}"))?;
    if url.host_str() != Some(HOST) {
        return Err(format!("not a Mangakakalot address: {path}"));
    }
    Ok(url.to_string())
}

/// The slug in `/manga/<slug>` (or `/manga/<slug>/<chapter>`).
fn slug_of(manga_url: &str) -> Res<&str> {
    let slug = manga_url.strip_prefix("/manga/").unwrap_or(manga_url).split(['/', '?']).next().unwrap_or_default();
    let ok = !slug.is_empty() && slug.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if ok { Ok(slug) } else { Err(format!("not a Mangakakalot manga: {manga_url}")) }
}

// =============================== Parsers ===============================

fn sel(css: &str) -> Selector {
    Selector::parse(css).expect("a valid selector")
}

fn text_of(element: ElementRef) -> String {
    element.text().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The path of an address on the site (`/manga/naruto`).
fn path_of(href: &str) -> String {
    Url::parse(BASE).and_then(|base| base.join(href)).map(|u| u.path().to_owned()).unwrap_or_else(|_| href.to_owned())
}

fn absolute(src: &str) -> String {
    Url::parse(BASE).and_then(|base| base.join(src)).map(|u| u.to_string()).unwrap_or_else(|_| src.to_owned())
}

/// A card of a listing: the link and title in its `h3`, the cover in its image.
fn card(element: ElementRef) -> Option<Manga> {
    let link = element.select(&sel("h3 a")).next()?;
    let title = text_of(link);
    if title.is_empty() {
        return None;
    }
    let image = element.select(&sel("img")).next();
    let cover = image.and_then(|i| i.value().attr("data-src").or(i.value().attr("src"))).map(absolute);
    Some(Manga { url: path_of(link.value().attr("href")?), title, author: None, description: None, genre: None, status: 0, thumbnail_url: cover })
}

/// Whether a pager has a page after the selected one: the link after `a.page_select` (or `page-select`) that is not "Last".
fn has_next(doc: &Html) -> bool {
    doc.select(&sel("a.page_select, a.page-select")).any(|selected| {
        selected.next_siblings().filter_map(ElementRef::wrap).find(|e| e.value().name() == "a").is_some_and(|next| {
            let classes = next.value();
            !(classes.has_class("page_last", scraper::CaseSensitivity::CaseSensitive) || classes.has_class("page-last", scraper::CaseSensitivity::CaseSensitive))
        })
    })
}

/// A hot or latest listing.
pub fn parse_list(html: &str) -> MangasPage {
    let doc = Html::parse_document(html);
    let mangas = doc.select(&sel("div.list-comic-item-wrap, div.list-truyen-item-wrap")).filter_map(card).collect();
    MangasPage { mangas, has_next_page: has_next(&doc) }
}

pub fn parse_search(html: &str) -> MangasPage {
    let doc = Html::parse_document(html);
    let mangas = doc.select(&sel(".panel_story_list .story_item, div.list-truyen-item-wrap, div.list-comic-item-wrap")).filter_map(card).collect();
    MangasPage { mangas, has_next_page: has_next(&doc) }
}

/// The text a `li` holds after its label (`Author(s) : Unknown`), without the label.
fn after_label(doc: &Html, label: &str) -> Option<String> {
    doc.select(&sel("ul.manga-info-text li")).map(text_of).find_map(|line| line.strip_prefix(label).map(|rest| rest.trim_start_matches([' ', ':']).trim().to_owned())).filter(|v| !v.is_empty())
}

pub fn parse_details(html: &str) -> Res<Manga> {
    let doc = Html::parse_document(html);
    let title = doc.select(&sel("ul.manga-info-text h1")).next().map(text_of).ok_or("the manga page has no title")?;
    let status = match after_label(&doc, "Status").as_deref() {
        Some(s) if s.contains("Ongoing") => 1,
        Some(s) if s.contains("Completed") => 2,
        _ => 0,
    };
    let genres: Vec<String> = doc.select(&sel("li.genres a")).map(text_of).collect();
    // the summary is the text directly in the box (the heading "<title> summary:" is a child and stays out)
    let mut description = doc
        .select(&sel("#contentBox"))
        .next()
        .map(|b| b.children().filter_map(|c| if let Node::Text(t) = c.value() { Some(t.text.trim()) } else { None }).collect::<Vec<_>>().join(" ").trim().to_owned())
        .unwrap_or_default();
    if let Some(alternative) = doc.select(&sel(".story-alternative")).next().map(text_of) {
        let alternative = alternative.trim_start_matches("Alternative").trim_start_matches([' ', ':']).trim().to_owned();
        if !alternative.is_empty() {
            description = if description.is_empty() { format!("Alternative Name: {alternative}") } else { format!("{description}\n\nAlternative Name: {alternative}") };
        }
    }
    Ok(Manga {
        url: String::new(),
        title,
        author: after_label(&doc, "Author(s)").filter(|a| a != "Unknown"),
        description: Some(description).filter(|d| !d.is_empty()),
        genre: Some(genres.join(", ")).filter(|g| !g.is_empty()),
        status,
        thumbnail_url: doc.select(&sel("div.manga-info-pic img, span.info-image img")).next().and_then(|i| i.value().attr("src")).map(absolute),
    })
}

#[derive(Deserialize)]
struct ChaptersAnswer {
    #[serde(default)]
    success: bool,
    data: Option<ChaptersData>,
}

#[derive(Deserialize)]
struct ChaptersData {
    #[serde(default)]
    chapters: Vec<ApiChapter>,
}

#[derive(Deserialize)]
struct ApiChapter {
    chapter_name: Option<String>,
    chapter_slug: Option<String>,
    chapter_num: Option<f32>,
    updated_at: Option<String>,
}

/// The chapters of the API answer, in the order it gives (newest first).
pub fn parse_chapters(body: &str, manga_url: &str) -> Res<Vec<Chapter>> {
    let slug = slug_of(manga_url)?;
    let answer: ChaptersAnswer = serde_json::from_str(body).map_err(|e| format!("unexpected answer: {e}"))?;
    if !answer.success {
        return Ok(Vec::new());
    }
    Ok(answer
        .data
        .map(|d| d.chapters)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|c| {
            let chapter_slug = c.chapter_slug?;
            Some(Chapter {
                url: format!("/manga/{slug}/{chapter_slug}"),
                name: c.chapter_name.unwrap_or_else(|| "Chapter".to_owned()),
                date_upload: c.updated_at.as_deref().map_or(0, millis),
                chapter_number: c.chapter_num.unwrap_or(0.0),
                scanlator: "mangakakalot.gg".to_owned(),
            })
        })
        .collect())
}

/// An ISO 8601 time (`2026-10-14T18:54:18.000000Z`) as epoch milliseconds; 0 when it does not read.
pub fn millis(text: &str) -> i64 {
    let parse = || -> Option<i64> {
        let (date, rest) = text.split_once('T')?;
        let d: Vec<i64> = date.split('-').map(|p| p.parse().ok()).collect::<Option<_>>()?;
        let [y, m, day] = d.as_slice().try_into().ok()?;
        let t: Vec<i64> = rest.get(..8)?.split(':').map(|p| p.parse().ok()).collect::<Option<_>>()?;
        let [hh, mm, ss] = t.as_slice().try_into().ok()?;
        let (y, m) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let doy = (153 * m + 2) / 5 + day - 1;
        let days = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
        Some(((days * 24 + hh) * 3600 + mm * 60 + ss) * 1000)
    };
    parse().unwrap_or(0)
}

/// The array a chapter page's script names: `var <name> = ["a","b"];` (JSON, with `\/` for each `/`).
fn script_array(script: &str, name: &str) -> Vec<String> {
    let Some(start) = script.find(&format!("var {name} =")) else { return Vec::new() };
    let rest = &script[start..];
    let (Some(open), Some(close)) = (rest.find('['), rest.find("];")) else { return Vec::new() };
    serde_json::from_str(&rest[open..=close]).unwrap_or_default()
}

/// The page images of a chapter, in order: the image paths of its script on the first CDN, else the images in the reader.
pub fn parse_pages(html: &str) -> Vec<String> {
    let doc = Html::parse_document(html);
    let script: String = doc.select(&sel("script")).map(|s| s.text().collect::<String>()).filter(|s| s.contains("cdns")).collect::<Vec<_>>().join("\n");
    let cdns = script_array(&script, "cdns");
    let paths = script_array(&script, "chapterImages");
    if let (Some(cdn), false) = (cdns.first(), paths.is_empty()) {
        let cdn = cdn.trim_end_matches('/');
        return paths.iter().map(|p| format!("{cdn}/{}", p.trim_start_matches('/'))).collect();
    }
    doc.select(&sel("div.container-chapter-reader > img")).filter_map(|i| i.value().attr("src")).map(absolute).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const POPULAR: &str = include_str!("../fixtures/popular.html");
    const LATEST: &str = include_str!("../fixtures/latest.html");
    const SEARCH: &str = include_str!("../fixtures/search.html");
    const MANGA: &str = include_str!("../fixtures/manga.html");
    const CHAPTERS: &str = include_str!("../fixtures/chapters.json");
    const CHAPTER: &str = include_str!("../fixtures/chapter.html");

    #[test]
    fn the_hot_list_has_cards_and_a_next_page() {
        let page = parse_list(POPULAR);
        assert_eq!(page.mangas.len(), 5);
        assert!(page.has_next_page);
        assert_eq!((page.mangas[0].url.as_str(), page.mangas[0].title.as_str()), ("/manga/martial-peak", "Martial Peak"));
        assert!(page.mangas[0].thumbnail_url.as_deref().unwrap().starts_with("https://img-r1.2xstorage.com/thumb/"));
        assert!(!parse_list(LATEST).mangas.is_empty());
    }

    #[test]
    fn a_search_lists_the_story_items() {
        let page = parse_search(SEARCH);
        assert_eq!(page.mangas[0].title, "Naruto");
        assert_eq!(page.mangas[0].url, "/manga/naruto");
        assert_eq!(search_url(2, "One Piece: Gold!"), "https://www.mangakakalot.gg/search/story/one_piece_gold?page=2");
    }

    #[test]
    fn the_pager_means_more_only_after_the_selected_page() {
        let more = r#"<div class="group_page"><a class="page_select">1</a> <a href="/x?page=2">2</a> <a class="page_blue page_last" href="/x?page=9">Last(9)</a></div>"#;
        let last = r#"<div class="group_page"><a class="page_blue" href="/x?page=1">First(1)</a> <a href="/x?page=8">8</a> <a class="page_select">9</a></div>"#;
        let before_last = r#"<div class="group_page"><a class="page_select">8</a> <a class="page_blue page_last" href="/x?page=9">Last(9)</a></div>"#;
        assert!(has_next(&Html::parse_document(more)));
        assert!(!has_next(&Html::parse_document(last)));
        assert!(!has_next(&Html::parse_document(before_last)), "only the Last link follows");
    }

    #[test]
    fn the_manga_page_has_title_author_status_genres_summary_and_cover() {
        let manga = parse_details(MANGA).unwrap();
        assert_eq!(manga.title, "No More Regrets");
        assert_eq!((manga.status, manga.author), (1, None));
        assert_eq!(manga.genre.as_deref(), Some("Fantasy, Shoujo, Webtoons"));
        let description = manga.description.unwrap();
        assert!(description.starts_with("The husband she loved"), "{description}");
        assert!(description.contains("Alternative Name: 二度目の人生"));
        assert!(manga.thumbnail_url.unwrap().ends_with("/thumb/no-more-regrets.webp"));
    }

    #[test]
    fn the_chapters_come_from_the_api_with_dates() {
        let chapters = parse_chapters(CHAPTERS, "/manga/no-more-regrets").unwrap();
        assert_eq!(chapters.len(), 6);
        assert_eq!(chapters[0].name, "Chapter 65: Episode 65");
        assert_eq!(chapters[0].url, "/manga/no-more-regrets/chapter-65");
        assert_eq!(chapters[0].chapter_number, 65.0);
        assert_eq!(chapters[0].date_upload, millis("2026-10-14T18:54:18.000000Z"));
        assert_eq!(millis("1970-01-02T00:00:01.000000Z"), 86_401_000);
        assert_eq!(chapters_url("/manga/no-more-regrets").unwrap(), "https://www.mangakakalot.gg/api/manga/no-more-regrets/chapters?limit=-1");
        assert!(parse_chapters(r#"{"success":false}"#, "/manga/x").unwrap().is_empty());
    }

    #[test]
    fn the_pages_are_the_first_cdn_and_the_image_paths() {
        let pages = parse_pages(CHAPTER);
        assert_eq!(pages.len(), 6);
        assert_eq!(pages[0], "https://img-r1.2xstorage.com/no-more-regrets/65/0.webp");
        let reader_only = r#"<div class="container-chapter-reader"><img src="https://c.test/1.jpg"><img src="https://c.test/2.jpg"></div>"#;
        assert_eq!(parse_pages(reader_only), ["https://c.test/1.jpg", "https://c.test/2.jpg"]);
    }

    #[test]
    fn only_addresses_of_the_site_are_fetched() {
        assert_eq!(page("/manga/naruto").unwrap(), "https://www.mangakakalot.gg/manga/naruto");
        assert!(page("https://evil.test/x").is_err());
        assert!(chapters_url("/manga/../x").is_err());
        assert_eq!(query_slug("  Hello,  World! "), "hello_world");
    }
}
