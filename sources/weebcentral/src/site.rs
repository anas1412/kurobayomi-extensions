//! The pure part of the Weeb Central source: URLs and HTML parsers. Nothing here touches the network; `lib.rs` fetches
//! the pages. Port of `rust-sources/weebcentral/src/lib.rs`, which ports the keiyoushi `en/weebcentral` extension
//! (Apache-2.0). The parsers are tested over pages saved from the site on 2026-10-10 (`fixtures/`).

use scraper::{ElementRef, Html, Selector};
use url::Url;

use crate::{WireChapter, WireManga, WireMangasPage};

pub const BASE_URL: &str = "https://weebcentral.com";
/// The site always returns 32 entries; the `limit` parameter is ignored.
const PAGE_SIZE: i32 = 32;

// The status values of the app's manga model (kuro-source `model::manga`).
const UNKNOWN: i32 = 0;
const ONGOING: i32 = 1;
const COMPLETED: i32 = 2;
const CANCELLED: i32 = 5;
const ON_HIATUS: i32 = 6;

/// A site path (`/series/...`) as an absolute address. A path must start with `/`.
pub fn page(path: &str) -> Result<Url, String> {
    if !path.starts_with('/') {
        return Err(format!("{path:?} is not a site path"));
    }
    Url::parse(&format!("{BASE_URL}{path}")).map_err(|e| e.to_string())
}

/// `/search/data` for one page, with the default filters and the given sort ("Popularity", "Latest Updates" or
/// "Best Match"). The characters `!#:(),-` of the query become spaces, as the Kotlin does.
pub fn listing_url(page_number: i32, query: &str, sort: &str) -> Result<Url, String> {
    let text = query.replace(|c: char| "!#:(),-".contains(c), " ");
    let offset = page_number.max(1).saturating_sub(1).saturating_mul(PAGE_SIZE);
    let mut url = page("/search/data")?;
    url.query_pairs_mut()
        .append_pair("text", text.trim())
        .append_pair("sort", sort)
        .append_pair("order", "Descending")
        .append_pair("official", "Any")
        .append_pair("anime", "Any")
        .append_pair("adult", "Any")
        .append_pair("limit", &PAGE_SIZE.to_string())
        .append_pair("offset", &offset.to_string())
        .append_pair("display_mode", "Full Display");
    Ok(url)
}

/// The full chapter list of a series (`/series/{id}/{slug}` gives `/series/{id}/full-chapter-list`).
pub fn chapters_url(series: &str) -> Result<Url, String> {
    let id = page(series)?
        .path_segments()
        .and_then(|mut segments| segments.nth(1).map(str::to_owned))
        .filter(|id| !id.is_empty())
        .ok_or_else(|| format!("no series id in {series}"))?;
    page(&format!("/series/{id}/full-chapter-list"))
}

/// The long-strip page list of a chapter (`/chapters/{id}` gives `/chapters/{id}/images`).
pub fn pages_url(chapter: &str) -> Result<Url, String> {
    let base = page(chapter)?;
    Url::parse(&format!("{}/images?is_prev=False&reading_style=long_strip", base.as_str().trim_end_matches('/')))
        .map_err(|e| e.to_string())
}

/// The path, query and fragment of an absolute address: the key the app stores a manga or chapter under.
fn path_of(link: &Url) -> String {
    let mut out = link.path().to_owned();
    if let Some(query) = link.query() {
        out.push('?');
        out.push_str(query);
    }
    if let Some(fragment) = link.fragment() {
        out.push('#');
        out.push_str(fragment);
    }
    out
}

fn resolve(base: &Url, href: &str) -> Option<Url> {
    base.join(href.trim()).ok()
}

fn sel(css: &str) -> Selector {
    // The selectors are constants of this file, and the fixture tests parse every one of them.
    Selector::parse(css).unwrap_or_else(|e| panic!("selector {css:?}: {e:?}"))
}

/// Jsoup `text()`: the text of an element with every whitespace run (NBSP included) collapsed to one space.
fn text(element: ElementRef<'_>) -> String {
    element
        .text()
        .collect::<String>()
        .split(|c: char| matches!(c, ' ' | '\t' | '\n' | '\u{0C}' | '\r' | '\u{A0}'))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn first<'a>(scope: ElementRef<'a>, css: &str) -> Option<ElementRef<'a>> {
    scope.select(&sel(css)).next()
}

/// The element children of `element` with tag `tag` (the CSS `> tag` step).
fn children<'a>(element: ElementRef<'a>, tag: &str) -> Vec<ElementRef<'a>> {
    element.children().filter_map(ElementRef::wrap).filter(|e| e.value().name() == tag).collect()
}

fn attr(element: ElementRef<'_>, name: &str) -> Option<String> {
    element.value().attr(name).map(str::to_owned)
}

/// Kotlin `sourceImg`: the `srcset` of the first `<source>` with "small" made "normal", or else the first `<img>` `src`.
fn source_img(scope: ElementRef<'_>, base: &Url) -> Option<String> {
    if let Some(source) = first(scope, "source") {
        return attr(source, "srcset").map(|s| s.replace("small", "normal"));
    }
    first(scope, "img")
        .and_then(|img| attr(img, "src"))
        .and_then(|src| resolve(base, &src))
        .map(String::from)
}

/// The `li` elements under `scope` whose `strong` child contains one of `labels` (Jsoup
/// `li:has(strong:contains(label))`, case-insensitive as Jsoup is).
fn labelled<'a>(scope: ElementRef<'a>, labels: &[&str]) -> Vec<ElementRef<'a>> {
    let strong = sel("strong");
    let items = sel("li");
    scope
        .select(&items)
        .filter(|li| {
            let found = li.select(&strong).any(|s| {
                let label_text = text(s).to_lowercase();
                labels.iter().any(|l| label_text.contains(&l.to_lowercase()))
            });
            found
        })
        .collect()
}

/// The `li` elements nested in the labelled `li` elements of `scope`.
fn nested<'a>(scope: ElementRef<'a>, labels: &[&str]) -> Vec<ElementRef<'a>> {
    let items = sel("li");
    labelled(scope, labels).into_iter().flat_map(|item| item.select(&items).collect::<Vec<_>>()).collect()
}

/// A `/search/data` fragment: the entries and whether a "View More Results" button follows.
pub fn parse_listing(html: &str, base: &Url) -> Result<WireMangasPage, String> {
    let document = Html::parse_document(html);
    let root = document.root_element();
    let entries = sel("article > section > a");
    let title = sel("div:not([class]):last-child");
    let mut mangas = Vec::new();
    for link in root.select(&entries) {
        let href = attr(link, "href").and_then(|h| resolve(base, &h));
        let name = link.select(&title).next().map(text);
        let (Some(href), Some(name)) = (href, name) else {
            return Err("a listing entry has no link or title".into());
        };
        mangas.push(WireManga {
            url: path_of(&href),
            title: name,
            thumbnail_url: source_img(link, base),
            ..WireManga::default()
        });
    }
    Ok(WireMangasPage { mangas, has_next_page: first(root, "button").is_some() })
}

fn status_of(raw: Option<String>) -> i32 {
    match raw.map(|s| s.to_lowercase()).as_deref() {
        Some("ongoing") => ONGOING,
        Some("complete") => COMPLETED,
        Some("hiatus") => ON_HIATUS,
        Some("canceled") => CANCELLED,
        _ => UNKNOWN,
    }
}

/// A series page. `at` is the address the page was read from; the manga key is its path.
pub fn parse_details(html: &str, at: &Url) -> Result<WireManga, String> {
    let document = Html::parse_document(html);
    let root = document.root_element();
    let sections_sel = sel("section[x-data] > section");
    let sections: Vec<_> = root.select(&sections_sel).collect();
    let (Some(left), Some(right)) = (sections.first().copied(), sections.get(1).copied()) else {
        return Err("the series page has no info sections".into());
    };
    let links = sel("a");
    let join = |items: Vec<String>| (!items.is_empty()).then(|| items.join(", "));

    let author: Vec<String> = labelled(left, &["Author"])
        .into_iter()
        .flat_map(|li| children(li, "span"))
        .flat_map(|span| children(span, "a"))
        .map(text)
        .collect();
    let genre: Vec<String> = labelled(left, &["Tag", "Type"])
        .into_iter()
        .flat_map(|li| li.select(&links).map(text).collect::<Vec<_>>())
        .collect();
    let status = labelled(left, &["Status"]).first().and_then(|li| children(*li, "a").into_iter().next()).map(text);

    let title = first(right, "h1").map(text).ok_or("the series page has no title")?;
    let mut description = String::new();
    if let Some(p) = labelled(right, &["Description"]).first().and_then(|li| children(*li, "p").into_iter().next()) {
        description.push_str(&text(p).replace("NOTE: ", "\n\nNOTE: "));
    }

    let related = nested(right, &["Related Series"]);
    if !related.is_empty() {
        description.push_str("\n\nRelated Series(s):");
        for series in related {
            let Some(link) = first(series, "a") else { continue };
            let href = attr(link, "href").and_then(|h| resolve(at, &h)).map(String::from).unwrap_or_default();
            let relation = first(series, "span").map(text).unwrap_or_default();
            description.push_str(format!("\n- [{}]({href}) {relation}", text(link)).trim_end());
        }
    }
    let alternates = nested(right, &["Associated Name"]);
    if !alternates.is_empty() {
        description.push_str("\n\nAssociated Name(s):");
        for name in alternates {
            description.push_str(&format!("\n- {}", text(name)));
        }
    }
    let tracker_links = sel("span[data-tip] > a");
    let trackers: Vec<ElementRef> =
        labelled(root, &["Track"]).into_iter().flat_map(|li| li.select(&tracker_links).collect::<Vec<_>>()).collect();
    if !trackers.is_empty() {
        description.push_str("\n\nTracker(s):");
        for tracker in trackers {
            let name = tracker.parent().and_then(ElementRef::wrap).and_then(|p| attr(p, "data-tip")).unwrap_or_default();
            let href = attr(tracker, "href").and_then(|h| resolve(at, &h)).map(String::from).unwrap_or_default();
            description.push_str(&format!("\n- [{name}]({href})"));
        }
    }

    Ok(WireManga {
        url: path_of(at),
        title,
        author: join(author),
        genre: join(genre),
        status: status_of(status),
        thumbnail_url: source_img(left, at),
        description: Some(description),
        initialized: true,
        ..WireManga::default()
    })
}

/// True when a chapter name carries a season ("Season 2", "S2"): such a name restarts its numbering, so the Kotlin
/// numbers its chapters by position.
fn has_season(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.match_indices('s').any(|(i, _)| {
        let rest = &lower[i..];
        let rest = rest.strip_prefix("season").or_else(|| rest.strip_prefix('s')).unwrap_or("");
        rest.trim_start().starts_with(|c: char| c.is_ascii_digit())
    })
}

/// The full chapter list fragment, newest first. `base` is the address the fragment was read from.
pub fn parse_chapters(html: &str, base: &Url) -> Result<Vec<WireChapter>, String> {
    let document = Html::parse_document(html);
    let entries = sel("div[x-data] > a");
    let links: Vec<ElementRef> = document.root_element().select(&entries).collect();
    let total = links.len();
    let images = sel("img");
    links
        .into_iter()
        .enumerate()
        .map(|(index, link)| -> Result<WireChapter, String> {
            let name = first(link, "span.flex > span").map(text).ok_or("a chapter has no name")?;
            let href = attr(link, "href").and_then(|h| resolve(base, &h)).ok_or("a chapter has no link")?;
            let date_upload = first(link, "time[datetime]")
                .and_then(|time| attr(time, "datetime"))
                .and_then(|d| chrono::DateTime::parse_from_rfc3339(&d).ok())
                .map_or(0, |d| d.timestamp_millis());
            let official = link
                .select(&images)
                .any(|img| attr(img, "src").is_some_and(|s| s.to_lowercase().contains("official")));
            let chapter_number = if has_season(&name) { (total - index) as f32 } else { -1.0 };
            let scanlator = if official { "Official" } else { "Unknown" };
            Ok(WireChapter {
                url: path_of(&href),
                name,
                date_upload,
                chapter_number,
                scanlator: Some(scanlator.to_owned()),
            })
        })
        .collect()
}

/// The image addresses of a chapter's long-strip fragment, in order.
pub fn parse_pages(html: &str, base: &Url) -> Vec<String> {
    let document = Html::parse_document(html);
    // Jsoup's `[x-data~=scroll]` is a regex match, so this is a substring match in CSS.
    let images = sel("section[x-data*=scroll] > img");
    document
        .root_element()
        .select(&images)
        .filter_map(|img| attr(img, "src"))
        .filter_map(|src| resolve(base, &src))
        .map(String::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn season_matches_the_kotlin_regex() {
        for yes in ["Season 2 Chapter 1", "S2 - Ep 3", "s 10", "Chapters 5"] {
            assert!(has_season(yes), "{yes}");
        }
        for no in ["Chapter 12", "Special", "Side Story"] {
            assert!(!has_season(no), "{no}");
        }
    }

    #[test]
    fn popular_lists_32_series_with_covers_and_a_next_page() {
        let page = parse_listing(include_str!("../fixtures/popular.html"), &url("https://weebcentral.com/search/data")).unwrap();
        assert_eq!(page.mangas.len(), 32);
        assert!(page.has_next_page);
        let first = &page.mangas[0];
        assert_eq!(first.title, "One Piece");
        assert_eq!(first.url, "/series/01J76XY7E9FNDZ1DBBM6PBJPFK/One-Piece");
        assert_eq!(
            first.thumbnail_url.as_deref(),
            Some("https://temp.compsci88.com/cover/normal/01J76XY7E9FNDZ1DBBM6PBJPFK.webp")
        );
        assert!(page.mangas.iter().all(|m| m.url.starts_with("/series/") && !m.title.is_empty()));
    }

    #[test]
    fn latest_lists_a_full_page() {
        let page = parse_listing(include_str!("../fixtures/latest.html"), &url("https://weebcentral.com/search/data")).unwrap();
        assert_eq!(page.mangas.len(), 32);
        assert!(page.has_next_page);
    }

    #[test]
    fn a_short_search_has_no_next_page() {
        let page = parse_listing(include_str!("../fixtures/search.html"), &url("https://weebcentral.com/search/data")).unwrap();
        assert_eq!(page.mangas.len(), 5);
        assert!(!page.has_next_page);
        assert!(page.mangas.iter().all(|m| m.title.contains("One Piece")), "{:?}", page.mangas);
    }

    #[test]
    fn details_read_every_field() {
        let at = url("https://weebcentral.com/series/01J76XY7E9FNDZ1DBBM6PBJPFK/One-Piece");
        let m = parse_details(include_str!("../fixtures/details.html"), &at).unwrap();
        assert_eq!(m.url, "/series/01J76XY7E9FNDZ1DBBM6PBJPFK/One-Piece");
        assert_eq!(m.title, "One Piece");
        assert_eq!(m.author.as_deref(), Some("ODA Eiichiro"));
        assert_eq!(m.genre.as_deref(), Some("Action, Adventure, Comedy, Drama, Fantasy, Shounen, Manga"));
        assert_eq!(m.status, ONGOING);
        assert_eq!(
            m.thumbnail_url.as_deref(),
            Some("https://temp.compsci88.com/cover/normal/01J76XY7E9FNDZ1DBBM6PBJPFK.webp")
        );
        let d = m.description.unwrap();
        assert!(d.starts_with("As a child, Monkey D. Luffy"), "{d}");
        assert!(
            d.contains("\n\nRelated Series(s):\n- [Wanted!](https://weebcentral.com/series/01J76XY8NPW1EH8SFZEXX8ZCRT) (Alternate Story)"),
            "{d}"
        );
        assert!(
            d.contains("\n\nTracker(s):\n- [Official Source](https://www.viz.com/one-piece)\n- [AniList](https://anilist.co/manga/30013/ONE-PIECE/)"),
            "{d}"
        );
        assert!(m.initialized);
    }

    #[test]
    fn chapters_are_newest_first_with_dates_and_scanlators() {
        let base = url("https://weebcentral.com/series/01J76XY7E9FNDZ1DBBM6PBJPFK/full-chapter-list");
        let chapters = parse_chapters(include_str!("../fixtures/chapters.html"), &base).unwrap();
        assert_eq!(chapters.len(), 11, "the back-to-top link is not a chapter");
        let c = &chapters[0];
        assert_eq!(c.name, "Chapter 1195");
        assert_eq!(c.url, "/chapters/01M4FFVTBPS4RKXQWZ2REYGH3M");
        assert_eq!(c.date_upload, 1_791_521_319_286);
        assert_eq!(c.chapter_number, -1.0, "no season: the app parses the number from the name");
        assert_eq!(c.scanlator.as_deref(), Some("Unknown"));
        assert_eq!(chapters.last().unwrap().name, "Chapter 1");
        assert_eq!(chapters.last().unwrap().scanlator.as_deref(), Some("Official"));
    }

    #[test]
    fn pages_are_the_long_strip_images_in_order() {
        let base = url("https://weebcentral.com/chapters/01M4FFVTBPS4RKXQWZ2REYGH3M/images");
        let pages = parse_pages(include_str!("../fixtures/pages.html"), &base);
        assert_eq!(pages.len(), 16);
        assert_eq!(pages[0], "https://temp.compsci88.com/manga/One-Piece/1195-001.png");
        assert_eq!(pages[15], "https://temp.compsci88.com/manga/One-Piece/1195-016.png");
    }

    #[test]
    fn listing_urls_match_the_kotlin() {
        let popular = listing_url(1, "", "Popularity").unwrap();
        assert_eq!(
            popular.as_str(),
            "https://weebcentral.com/search/data?text=&sort=Popularity&order=Descending&official=Any&anime=Any&adult=Any&limit=32&offset=0&display_mode=Full+Display"
        );
        let latest = listing_url(3, "", "Latest Updates").unwrap();
        assert!(latest.as_str().contains("sort=Latest+Updates") && latest.as_str().contains("offset=64"), "{latest}");
        let search = listing_url(1, "One Piece: Party (x)", "Best Match").unwrap();
        assert!(
            search.as_str().starts_with("https://weebcentral.com/search/data?text=One+Piece++Party++x&sort=Best+Match&"),
            "{search}"
        );
    }

    #[test]
    fn operation_urls() {
        assert_eq!(
            chapters_url("/series/01J76XY7E9FNDZ1DBBM6PBJPFK/One-Piece").unwrap().as_str(),
            "https://weebcentral.com/series/01J76XY7E9FNDZ1DBBM6PBJPFK/full-chapter-list"
        );
        assert_eq!(
            pages_url("/chapters/01M4FFVTBPS4RKXQWZ2REYGH3M").unwrap().as_str(),
            "https://weebcentral.com/chapters/01M4FFVTBPS4RKXQWZ2REYGH3M/images?is_prev=False&reading_style=long_strip"
        );
        assert!(page("https://evil.test/series/ABC/X").is_err(), "an absolute address is not a site path");
    }
}
