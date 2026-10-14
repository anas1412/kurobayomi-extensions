//! The MangaPill site (<https://mangapill.com>) as pure functions: URLs and one parser per page. Ported from the keiyoushi
//! `en/mangapill` extension (Apache-2.0). Without the filters (genre, status, type).

use scraper::{ElementRef, Html, Selector};
use serde::Serialize;
use url::Url;

pub type Res<T> = Result<T, String>;

pub const BASE: &str = "https://mangapill.com";

// ============================ Output (wire) ============================

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manga {
    pub url: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    /// SManga status: 0 unknown, 1 ongoing, 2 completed, 5 cancelled, 6 on hiatus.
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
}

// ================================ URLs =================================

pub fn home_url() -> String {
    format!("{BASE}/")
}

pub fn latest_url() -> String {
    format!("{BASE}/chapters")
}

pub fn search_url(page: i32, query: &str) -> String {
    let q: String = url::form_urlencoded::byte_serialize(query.trim().as_bytes()).collect();
    format!("{BASE}/search?page={}&q={q}", page.max(1))
}

/// A page of the site from a stored address (`/manga/...`, `/chapters/...`), refused when it is another site's.
pub fn page(path: &str) -> Res<String> {
    let url = Url::parse(BASE).and_then(|base| base.join(path)).map_err(|e| format!("invalid address {path}: {e}"))?;
    if url.host_str() != Some("mangapill.com") {
        return Err(format!("not a MangaPill address: {path}"));
    }
    Ok(url.to_string())
}

// =============================== Parsers ===============================

fn sel(css: &str) -> Selector {
    Selector::parse(css).expect("a valid selector")
}

fn text_of(element: ElementRef) -> String {
    element.text().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The path of an address on the site (`/manga/2/one-piece`).
fn path_of(href: &str) -> String {
    Url::parse(BASE).and_then(|base| base.join(href)).map(|u| u.path().to_owned()).unwrap_or_else(|_| href.to_owned())
}

/// A manga card: a cover, a link to the manga and its title (the Kotlin `latestUpdatesFromElement`).
fn card(element: ElementRef) -> Option<Manga> {
    let link = element.select(&sel("a[href^='/manga/']")).next()?;
    let title = element.select(&sel("div.line-clamp-2")).next().map(text_of).filter(|t| !t.is_empty())?;
    Some(Manga {
        url: path_of(link.value().attr("href")?),
        title,
        description: None,
        genre: None,
        status: 0,
        thumbnail_url: element.select(&sel("img")).next().and_then(|i| i.value().attr("data-src")).map(str::to_owned),
    })
}

fn cards(doc: &Html, container: &str) -> Vec<Manga> {
    doc.select(&sel(container)).filter_map(card).collect()
}

/// The trending titles of the home page: the site has no popular list.
pub fn parse_popular(html: &str) -> MangasPage {
    let doc = Html::parse_document(html);
    // the grid under the "Trending Mangas" heading
    let trending = doc.select(&sel("h4")).find(|h| text_of(*h).contains("Trending")).and_then(|h| h.parent()?.parent()).and_then(ElementRef::wrap);
    let mangas = trending
        .map(|section| section.select(&sel(".grid > div:not([class])")).filter_map(card).collect())
        .unwrap_or_default();
    MangasPage { mangas, has_next_page: false }
}

/// The latest chapters page, as manga cards.
pub fn parse_latest(html: &str) -> MangasPage {
    MangasPage { mangas: cards(&Html::parse_document(html), ".grid > div:not([class])"), has_next_page: false }
}

pub fn parse_search(html: &str) -> MangasPage {
    let doc = Html::parse_document(html);
    // the Kotlin takes any `a.btn.btn-sm`, which on a last page is its "Previous" link: only "Next" counts here
    let has_next_page = doc.select(&sel("a.btn.btn-sm")).any(|a| text_of(a) == "Next");
    MangasPage { mangas: cards(&doc, ".grid > div:not([class])"), has_next_page }
}

fn status_of(text: &str) -> i32 {
    let text = text.to_lowercase();
    if text.contains("publishing") {
        1
    } else if text.contains("finished") {
        2
    } else if text.contains("hiatus") {
        6
    } else if text.contains("discontinued") {
        5
    } else {
        0
    }
}

/// The value under a label of the details grid (`<label>Status</label><div>publishing</div>`).
fn labelled(doc: &Html, label: &str) -> Option<String> {
    doc.select(&sel("label")).find(|l| text_of(*l) == label).and_then(|l| l.next_siblings().filter_map(ElementRef::wrap).next()).map(text_of)
}

pub fn parse_details(html: &str) -> Res<Manga> {
    let doc = Html::parse_document(html);
    let title = doc.select(&sel("h1")).next().map(text_of).ok_or("the manga page has no title")?;
    let genres: Vec<String> = doc.select(&sel("a[href*=genre]")).map(text_of).collect();
    Ok(Manga {
        url: String::new(),
        title,
        description: doc.select(&sel("h1")).next().and_then(|h| h.parent()?.next_siblings().filter_map(ElementRef::wrap).find_map(|d| d.select(&sel("p")).next())).map(text_of).filter(|d| !d.is_empty()),
        genre: Some(genres.join(", ")).filter(|g| !g.is_empty()),
        status: status_of(&labelled(&doc, "Status").unwrap_or_default()),
        thumbnail_url: doc.select(&sel("img[data-src]")).next().and_then(|i| i.value().attr("data-src")).map(str::to_owned),
    })
}

/// The chapters of a manga page, newest first as the site lists them. The site gives no dates.
pub fn parse_chapters(html: &str) -> Vec<Chapter> {
    let doc = Html::parse_document(html);
    doc.select(&sel("#chapters a[href^='/chapters/']"))
        .map(|a| {
            let name = text_of(a);
            Chapter { url: path_of(a.value().attr("href").unwrap_or_default()), chapter_number: number_in(&name), name, date_upload: 0 }
        })
        .collect()
}

/// The number in a chapter name (`Chapter 1195`, `Chapter 12.5`); -1 when there is none.
fn number_in(name: &str) -> f32 {
    name.split_whitespace().rev().find_map(|word| word.parse::<f32>().ok()).unwrap_or(-1.0)
}

/// The page images of a chapter, in order.
pub fn parse_pages(html: &str) -> Vec<String> {
    Html::parse_document(html).select(&sel("picture img")).filter_map(|img| img.value().attr("data-src")).map(str::to_owned).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: &str = include_str!("../fixtures/home.html");
    const LATEST: &str = include_str!("../fixtures/chapters.html");
    const SEARCH: &str = include_str!("../fixtures/search.html");
    const MANGA: &str = include_str!("../fixtures/manga.html");
    const CHAPTER: &str = include_str!("../fixtures/chapter.html");

    #[test]
    fn the_home_page_gives_the_trending_cards() {
        let page = parse_popular(HOME);
        assert!(page.mangas.len() >= 3 && !page.has_next_page);
        assert_eq!((page.mangas[0].url.as_str(), page.mangas[0].title.as_str()), ("/manga/1/berserk", "Berserk"));
        assert!(page.mangas[0].thumbnail_url.as_deref().unwrap().starts_with("https://cdn."));
    }

    #[test]
    fn the_latest_chapters_page_lists_manga_cards() {
        let page = parse_latest(LATEST);
        assert!(page.mangas.len() >= 3, "{}", page.mangas.len());
        assert!(page.mangas.iter().all(|m| m.url.starts_with("/manga/") && !m.title.is_empty()));
    }

    #[test]
    fn a_search_lists_cards_and_a_next_link_means_more() {
        let page = parse_search(SEARCH);
        assert_eq!(page.mangas[0].title, "One Piece");
        assert_eq!(page.mangas[0].url, "/manga/2/one-piece");
        assert!(!page.has_next_page, "the fixture was cut before its Next link");
        let more = r#"<div class="grid"><div><a href="/manga/1/x"><img data-src="i"></a><div class="line-clamp-2">X</div></div></div><a href="/search?page=2" class="btn btn-sm">Next</a>"#;
        assert!(parse_search(more).has_next_page);
        assert!(!parse_search(r#"<a href="/search?page=1" class="btn btn-sm">Previous</a>"#).has_next_page, "Previous is not more");
        assert_eq!(search_url(2, "one piece"), "https://mangapill.com/search?page=2&q=one+piece");
    }

    #[test]
    fn the_manga_page_has_title_summary_genres_status_and_cover() {
        let manga = parse_details(MANGA).unwrap();
        assert_eq!(manga.title, "One Piece");
        assert_eq!(manga.status, 1);
        assert!(manga.genre.unwrap().contains("Action, Adventure"));
        assert!(manga.description.unwrap().starts_with("Gol D. Roger"));
        assert!(manga.thumbnail_url.unwrap().contains("/mangapill/i/2."));
    }

    #[test]
    fn the_chapters_are_listed_with_their_numbers_and_no_dates() {
        let chapters = parse_chapters(MANGA);
        assert!(chapters.len() >= 30);
        assert_eq!(chapters[0].name, "Chapter 1195");
        assert_eq!(chapters[0].url, "/chapters/2-11195000/one-piece-chapter-1195");
        assert_eq!((chapters[0].chapter_number, chapters[0].date_upload), (1195.0, 0));
        assert_eq!(number_in("Chapter 12.5"), 12.5);
        assert_eq!(number_in("Extra"), -1.0);
    }

    #[test]
    fn the_chapter_page_gives_its_images_in_order() {
        let pages = parse_pages(CHAPTER);
        assert!(pages.len() >= 4 && pages.iter().all(|p| p.starts_with("https://")), "{pages:?}");
    }

    #[test]
    fn only_addresses_of_the_site_are_fetched() {
        assert_eq!(page("/manga/2/one-piece").unwrap(), "https://mangapill.com/manga/2/one-piece");
        assert!(page("https://evil.test/x").is_err());
        assert_eq!(status_of("On Hiatus"), 6);
    }
}
