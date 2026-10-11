//! Every operation over pages saved from weebcentral.com on 2026-10-10 (no network).

use kuro_source::model::manga;
use kuro_source::{Filter, FilterList, TriStateState};
use kuro_src_weebcentral::*;
use url::Url;

fn fixture(name: &str) -> String {
    std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)).expect("fixture")
}

fn url(s: &str) -> Url {
    Url::parse(s).unwrap()
}

#[test]
fn popular_lists_32_series_with_covers_and_a_next_page() {
    let page = parse_listing(&fixture("popular.html"), &url("https://weebcentral.com/search/data")).unwrap();
    assert_eq!(page.mangas.len(), 32);
    assert!(page.has_next_page);
    let first = &page.mangas[0];
    assert_eq!(first.title, "One Piece");
    assert_eq!(first.url, "/series/01J76XY7E9FNDZ1DBBM6PBJPFK/One-Piece");
    assert_eq!(first.thumbnail_url.as_deref(), Some("https://temp.compsci88.com/cover/normal/01J76XY7E9FNDZ1DBBM6PBJPFK.webp"));
    assert!(page.mangas.iter().all(|m| m.url.starts_with("/series/") && !m.title.is_empty()));
}

#[test]
fn latest_lists_a_full_page() {
    let page = parse_listing(&fixture("latest.html"), &url("https://weebcentral.com/search/data")).unwrap();
    assert_eq!(page.mangas.len(), 32);
    assert!(page.has_next_page);
}

#[test]
fn a_short_search_has_no_next_page() {
    let page = parse_listing(&fixture("search.html"), &url("https://weebcentral.com/search/data")).unwrap();
    assert_eq!(page.mangas.len(), 5);
    assert!(!page.has_next_page);
    assert!(page.mangas.iter().all(|m| m.title.contains("One Piece")), "{:?}", page.mangas);
}

#[test]
fn details_read_every_field() {
    let at = url("https://weebcentral.com/series/01J76XY7E9FNDZ1DBBM6PBJPFK/One-Piece");
    let m = parse_details(&fixture("details.html"), &at).unwrap();
    assert_eq!(m.url, "/series/01J76XY7E9FNDZ1DBBM6PBJPFK/One-Piece");
    assert_eq!(m.title, "One Piece");
    assert_eq!(m.author.as_deref(), Some("ODA Eiichiro"));
    assert_eq!(m.genre.as_deref(), Some("Action, Adventure, Comedy, Drama, Fantasy, Shounen, Manga"));
    assert_eq!(m.status, manga::ONGOING);
    assert_eq!(m.thumbnail_url.as_deref(), Some("https://temp.compsci88.com/cover/normal/01J76XY7E9FNDZ1DBBM6PBJPFK.webp"));
    let d = m.description.unwrap();
    assert!(d.starts_with("As a child, Monkey D. Luffy"), "{d}");
    assert!(d.contains("\n\nRelated Series(s):\n- [Wanted!](https://weebcentral.com/series/01J76XY8NPW1EH8SFZEXX8ZCRT) (Alternate Story)"), "{d}");
    assert!(d.contains("\n\nTracker(s):\n- [Official Source](https://www.viz.com/one-piece)\n- [AniList](https://anilist.co/manga/30013/ONE-PIECE/)"), "{d}");
    assert!(m.initialized);
}

#[test]
fn chapters_are_newest_first_with_dates_and_scanlators() {
    let chapters = parse_chapters(&fixture("chapters.html"), &url("https://weebcentral.com/series/01J76XY7E9FNDZ1DBBM6PBJPFK/full-chapter-list")).unwrap();
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
    let pages = parse_pages(&fixture("pages.html"), &url("https://weebcentral.com/chapters/01M4FFVTBPS4RKXQWZ2REYGH3M/images"));
    assert_eq!(pages.len(), 16);
    assert_eq!(pages[0], "https://temp.compsci88.com/manga/One-Piece/1195-001.png");
    assert_eq!(pages[15], "https://temp.compsci88.com/manga/One-Piece/1195-016.png");
}

#[test]
fn listing_urls_match_the_kotlin() {
    let popular = search_url(1, "", &filters_sorted_by("Popularity")).unwrap();
    assert_eq!(
        popular.as_str(),
        "https://weebcentral.com/search/data?text=&sort=Popularity&order=Descending&official=Any&anime=Any&adult=Any&limit=32&offset=0&display_mode=Full+Display"
    );
    let latest = search_url(3, "", &filters_sorted_by("Latest Updates")).unwrap();
    assert!(latest.as_str().contains("sort=Latest+Updates") && latest.as_str().contains("offset=64"), "{latest}");
    let empty = search_url(1, "One Piece: Party (x)", &FilterList::default()).unwrap();
    assert!(empty.as_str().starts_with("https://weebcentral.com/search/data?text=One+Piece++Party++x&sort=Best+Match&"), "{empty}");
}

#[test]
fn filters_add_their_parameters() {
    let mut filters = filters_sorted_by("Alphabet");
    for f in &mut filters.0 {
        match f {
            Filter::Text { state, .. } => *state = "ODA Eiichiro".into(),
            Filter::Group { name, state } if name == "Series Status" => {
                if let Filter::CheckBox { state, .. } = &mut state[0] {
                    *state = true;
                }
            }
            Filter::Group { name, state } if name == "Tags" => {
                if let Filter::TriState { state, .. } = &mut state[0] {
                    *state = TriStateState::Include;
                }
                if let Filter::TriState { state, .. } = &mut state[10] {
                    *state = TriStateState::Exclude;
                }
            }
            _ => {}
        }
    }
    let u = search_url(1, "", &filters).unwrap();
    let q = u.query().unwrap();
    for part in ["sort=Alphabet", "author=ODA+Eiichiro", "included_status=Ongoing", "included_tag=Action", "excluded_tag=Hentai"] {
        assert!(q.contains(part), "{part} missing from {q}");
    }
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
    assert_eq!(series_path("https://weebcentral.com/series/ABC/Some-Title?x=1").as_deref(), Some("/series/ABC/Some-Title"));
    assert_eq!(series_path("https://weebcentral.com/chapters/ABC"), None);
    assert_eq!(series_path("https://evil.test/series/ABC/X"), None);
}
