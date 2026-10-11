//! Weeb Central (<https://weebcentral.com>), a built-in manga source ported from the keiyoushi `en/weebcentral`
//! extension (Apache-2.0). The site serves htmx fragments: `/search/data` for listings, the series page for details,
//! `/series/{id}/full-chapter-list` for chapters and `/chapters/{id}/images` for pages.
//!
//! The parsers are pure functions over an HTML string ([`parse_listing`], [`parse_details`], [`parse_chapters`],
//! [`parse_pages`]) and the URLs come from [`search_url`], so tests run over saved pages. [`WeebCentral`] is the only
//! network path: the one host, one request at a time, two seconds apart (the Kotlin `rateLimit(1, 2.seconds)`).

use std::time::Duration;

use async_trait::async_trait;
use http::header::REFERER;
use http::{HeaderMap, HeaderValue};
use kuro_net::NetworkClient;
use kuro_source::model::{MangasPage, Page, SChapter, SManga, manga};
use kuro_source::{
    CatalogueSource, Filter, FilterList, MangaSource, MangaUriType, ResolvableSource, SourceError, TriStateState,
};
use kuro_source_http::html::{parse_selector, resolve, text};
use kuro_source_http::http_source::{HttpCore, default_headers};
use kuro_source_http::response::HttpResponse;
use kuro_source_http::urls::url_without_domain;
use kuro_source_http::{map_net_error, request};
use scraper::{ElementRef, Html, Selector};
use tokio::sync::Mutex;
use tokio::time::Instant;
use url::Url;

pub const NAME: &str = "Weeb Central";
pub const LANG: &str = "en";
pub const BASE_URL: &str = "https://weebcentral.com";
const HOST: &str = "weebcentral.com";
/// The `limit` parameter is ignored by the site, which always returns 32 entries.
const FETCH_LIMIT: i32 = 32;
const INTERVAL: Duration = Duration::from_secs(2);

/// The source. Build it with [`source`].
pub struct WeebCentral {
    core: HttpCore,
    headers: HeaderMap,
    last_done: Mutex<Option<Instant>>,
}

/// The source over the app's client.
pub fn source(client: NetworkClient) -> WeebCentral {
    let core = HttpCore::new(client);
    let mut headers = default_headers(&core);
    headers.insert(REFERER, HeaderValue::from_static("https://weebcentral.com/"));
    WeebCentral { core, headers, last_done: Mutex::new(None) }
}

impl WeebCentral {
    /// The headers the reader should send with page images (Kotlin `imageRequest`). The user agent is the client's.
    pub fn image_headers(&self) -> Vec<(String, String)> {
        vec![("Referer".into(), format!("{BASE_URL}/")), ("Accept".into(), "image/avif,image/webp,*/*".into())]
    }

    /// Fetches a page of the site. Another host is refused before the request, and a redirect to one after it.
    async fn get(&self, url: &Url) -> Result<(String, Url), SourceError> {
        check_host(url)?;
        let request = request::get(url.as_str(), &self.headers)?;
        let mut last = self.last_done.lock().await;
        if let Some(done) = *last {
            tokio::time::sleep_until(done + INTERVAL).await;
        }
        let outcome = match self.core.client().execute(request).await.map_err(map_net_error) {
            Ok(response) => HttpResponse::read(response).await,
            Err(err) => Err(err),
        };
        *last = Some(Instant::now());
        drop(last);
        let response = outcome?;
        check_host(&response.url)?;
        Ok((response.text(), response.url))
    }

    async fn list(&self, page: i32, query: &str, filters: &FilterList) -> Result<MangasPage, SourceError> {
        let url = search_url(page, query, filters)?;
        let (html, at) = self.get(&url).await?;
        parse_listing(&html, &at)
    }

    async fn details(&self, path: &str) -> Result<SManga, SourceError> {
        let (html, at) = self.get(&absolute(path)?).await?;
        parse_details(&html, &at)
    }
}

fn check_host(url: &Url) -> Result<(), SourceError> {
    if url.scheme() == "https" && url.host_str() == Some(HOST) {
        Ok(())
    } else {
        Err(SourceError::Url(format!("{url} is not on {HOST}")))
    }
}

fn absolute(path: &str) -> Result<Url, SourceError> {
    Url::parse(&format!("{BASE_URL}{path}")).map_err(|e| SourceError::Url(e.to_string()))
}

// ================================ Filters =================================

const SORTS: [&str; 6] = ["Best Match", "Alphabet", "Popularity", "Subscribers", "Recently Added", "Latest Updates"];
const ANY_TRUE_FALSE: [&str; 3] = ["Any", "True", "False"];
/// Select filters: name, query parameter, values (the label is the value).
const SELECTS: [(&str, &str, &[&str]); 5] = [
    ("Sort", "sort", &SORTS),
    ("Sort Order", "order", &["Descending", "Ascending"]),
    ("Official Translation", "official", &ANY_TRUE_FALSE),
    ("Anime Adaptation", "anime", &ANY_TRUE_FALSE),
    ("Adult Content", "adult", &ANY_TRUE_FALSE),
];
const AUTHOR: &str = "Author (Case-sensitive)";
/// Check-box groups: name, query parameter, options.
const CHECK_GROUPS: [(&str, &str, &[&str]); 2] = [
    ("Series Status", "included_status", &["Ongoing", "Complete", "Hiatus", "Canceled"]),
    ("Series Type", "included_type", &["Manga", "Manhwa", "Manhua", "OEL"]),
];
const TAGS: &str = "Tags";
const TAG_VALUES: [&str; 38] = [
    "Action", "Adult", "Adventure", "Comedy", "Doujinshi", "Drama", "Ecchi", "Fantasy", "Gender Bender", "Harem",
    "Hentai", "Historical", "Horror", "Isekai", "Josei", "Lolicon", "Martial Arts", "Mature", "Mecha", "Mystery",
    "Psychological", "Romance", "School Life", "Sci-fi", "Seinen", "Shotacon", "Shoujo", "Shoujo Ai", "Shounen",
    "Shounen Ai", "Slice of Life", "Smut", "Sports", "Supernatural", "Tragedy", "Yaoi", "Yuri", "Other",
];

/// The Kotlin `defaultFilterList(SortFilter(sort))`: every filter at its default, the sort on `sort`.
pub fn filters_sorted_by(sort: &str) -> FilterList {
    let mut list: Vec<Filter> = SELECTS
        .iter()
        .map(|(name, _, values)| {
            let index = if *name == "Sort" { values.iter().position(|v| *v == sort).unwrap_or(0) } else { 0 };
            Filter::select_at(*name, owned(values), i32::try_from(index).unwrap_or(0))
        })
        .collect();
    list.push(Filter::text(AUTHOR));
    for (name, _, options) in CHECK_GROUPS {
        list.push(Filter::group(name, options.iter().map(|o| Filter::check_box(*o)).collect()));
    }
    list.push(Filter::group(TAGS, TAG_VALUES.iter().map(|t| Filter::tri_state(*t)).collect()));
    FilterList(list)
}

fn owned(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| (*v).to_owned()).collect()
}

/// The query parameters the filters add, in list order (Kotlin `UriFilter.addToUri`). Unknown filters are ignored.
fn filter_params(filters: &FilterList) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    for filter in filters.iter() {
        match filter {
            Filter::Select { name, .. } => {
                if let (Some((_, param, _)), Some(value)) =
                    (SELECTS.iter().find(|(n, _, _)| n == name), filter.select_value())
                {
                    out.push((*param, value.to_owned()));
                }
            }
            Filter::Text { name, state } if name == AUTHOR && !state.is_empty() => out.push(("author", state.clone())),
            Filter::Group { name, state } if name == TAGS => {
                for tag in state {
                    if let Filter::TriState { name, state } = tag {
                        match state {
                            TriStateState::Include => out.push(("included_tag", name.clone())),
                            TriStateState::Exclude => out.push(("excluded_tag", name.clone())),
                            TriStateState::Ignore => {}
                        }
                    }
                }
            }
            Filter::Group { name, state } => {
                if let Some((_, param, _)) = CHECK_GROUPS.iter().find(|(n, _, _)| n == name) {
                    for option in state {
                        if let Filter::CheckBox { name, state: true } = option {
                            out.push((*param, name.clone()));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// `/search/data` for one page. An empty filter list means the default list (sort "Best Match"). The characters
/// `!#:(),-` of the query become spaces, as the Kotlin does.
pub fn search_url(page: i32, query: &str, filters: &FilterList) -> Result<Url, SourceError> {
    let defaults;
    let filters = if filters.is_empty() {
        defaults = filters_sorted_by("");
        &defaults
    } else {
        filters
    };
    let mut url = absolute("/search/data")?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("text", query.replace(|c| "!#:(),-".contains(c), " ").trim());
        for (param, value) in filter_params(filters) {
            pairs.append_pair(param, &value);
        }
        pairs.append_pair("limit", &FETCH_LIMIT.to_string());
        pairs.append_pair("offset", &((page.max(1) - 1) * FETCH_LIMIT).to_string());
        pairs.append_pair("display_mode", "Full Display");
    }
    Ok(url)
}

/// The page-list address of a chapter path (`/chapters/{id}`).
pub fn pages_url(chapter_path: &str) -> Result<Url, SourceError> {
    let mut url = absolute(chapter_path)?;
    url.path_segments_mut().map_err(|()| SourceError::Url(chapter_path.to_owned()))?.pop_if_empty().push("images");
    url.query_pairs_mut().append_pair("is_prev", "False").append_pair("reading_style", "long_strip");
    Ok(url)
}

/// The full chapter list address of a series path (`/series/{id}/{slug}`).
pub fn chapters_url(series_path: &str) -> Result<Url, SourceError> {
    let id = absolute(series_path)?
        .path_segments()
        .and_then(|mut s| s.nth(1).map(str::to_owned))
        .filter(|id| !id.is_empty())
        .ok_or_else(|| SourceError::Url(format!("no series id in {series_path}")))?;
    absolute(&format!("/series/{id}/full-chapter-list"))
}

// ================================= Parsers ================================

fn sel(css: &str) -> Selector {
    // The selectors are constants of this file; a test parses every one of them.
    parse_selector(css).unwrap_or_else(|e| panic!("{e}"))
}

fn first<'a>(scope: ElementRef<'a>, css: &str) -> Option<ElementRef<'a>> {
    scope.select(&sel(css)).next()
}

/// The element children of `element` with tag `name` (a CSS `> name` step).
fn children<'a>(element: ElementRef<'a>, name: &'a str) -> impl Iterator<Item = ElementRef<'a>> + 'a {
    element.children().filter_map(ElementRef::wrap).filter(move |e| e.value().name() == name)
}

fn attr(element: ElementRef<'_>, name: &str) -> Option<String> {
    element.value().attr(name).map(str::to_owned)
}

/// Kotlin `sourceImg`: the `srcset` of the first `<source>` with "small" made "normal", or the first `<img>` `src`.
fn source_img(scope: ElementRef<'_>, base: &Url) -> Option<String> {
    if let Some(source) = first(scope, "source") {
        return attr(source, "srcset").map(|s| s.replace("small", "normal"));
    }
    first(scope, "img").and_then(|img| img.value().attr("src")).and_then(|src| resolve(base, src))
}

/// The `li` elements under `scope` whose `strong` child contains one of `labels` (Jsoup
/// `li:has(strong:contains(label))`, case-insensitive as Jsoup is).
fn labelled<'a>(scope: ElementRef<'a>, labels: &[&str]) -> Vec<ElementRef<'a>> {
    let strong = sel("strong");
    scope
        .select(&sel("li"))
        .filter(|li| {
            li.select(&strong).any(|s| {
                let t = text(s).to_lowercase();
                labels.iter().any(|l| t.contains(&l.to_lowercase()))
            })
        })
        .collect()
}

/// A `/search/data` fragment: the entries and whether a "View More Results" button follows.
pub fn parse_listing(html: &str, base: &Url) -> Result<MangasPage, SourceError> {
    let document = Html::parse_document(html);
    let root = document.root_element();
    let title = sel("div:not([class]):last-child");
    let mut mangas = Vec::new();
    for link in root.select(&sel("article > section > a")) {
        let href = link.value().attr("href").and_then(|h| resolve(base, h));
        let name = link.select(&title).next().map(text);
        let (Some(href), Some(name)) = (href, name) else {
            return Err(SourceError::Parse("a listing entry has no link or title".into()));
        };
        mangas.push(SManga { thumbnail_url: source_img(link, base), ..SManga::new(url_without_domain(&href), name) });
    }
    Ok(MangasPage { mangas, has_next_page: first(root, "button").is_some() })
}

fn parse_status(raw: Option<String>) -> i32 {
    match raw.map(|s| s.to_lowercase()).as_deref() {
        Some("ongoing") => manga::ONGOING,
        Some("complete") => manga::COMPLETED,
        Some("hiatus") => manga::ON_HIATUS,
        Some("canceled") => manga::CANCELLED,
        _ => manga::UNKNOWN,
    }
}

/// A series page. `at` is the final address of the response; the manga URL is its path.
pub fn parse_details(html: &str, at: &Url) -> Result<SManga, SourceError> {
    let document = Html::parse_document(html);
    let root = document.root_element();
    let sections: Vec<_> = root.select(&sel("section[x-data] > section")).collect();
    let (Some(left), Some(right)) = (sections.first().copied(), sections.get(1).copied()) else {
        return Err(SourceError::Parse("the series page has no info sections".into()));
    };
    let a = sel("a");
    let join = |items: Vec<String>| (!items.is_empty()).then(|| items.join(", "));

    let author = labelled(left, &["Author"])
        .into_iter()
        .flat_map(|li| children(li, "span").flat_map(|span| children(span, "a").map(text)).collect::<Vec<_>>())
        .collect();
    let genre = labelled(left, &["Tag", "Type"]).into_iter().flat_map(|li| li.select(&a).map(text).collect::<Vec<_>>()).collect();
    let status = labelled(left, &["Status"]).first().and_then(|li| children(*li, "a").next()).map(text);

    let title = first(right, "h1").map(text).ok_or_else(|| SourceError::Parse("the series page has no title".into()))?;
    let mut description = String::new();
    if let Some(p) = labelled(right, &["Description"]).first().and_then(|li| children(*li, "p").next()) {
        description.push_str(&text(p).replace("NOTE: ", "\n\nNOTE: "));
    }
    let nested = |labels: &[&str]| -> Vec<ElementRef<'_>> {
        labelled(right, labels).into_iter().flat_map(|li| li.select(&sel("li")).collect::<Vec<_>>()).collect()
    };
    let related = nested(&["Related Series"]);
    if !related.is_empty() {
        description.push_str("\n\nRelated Series(s):");
        for series in related {
            let Some(link) = first(series, "a") else { continue };
            let href = link.value().attr("href").and_then(|h| resolve(at, h)).unwrap_or_default();
            let relation = first(series, "span").map(text).unwrap_or_default();
            description.push_str(format!("\n- [{}]({href}) {relation}", text(link)).trim_end());
        }
    }
    let alternates = nested(&["Associated Name"]);
    if !alternates.is_empty() {
        description.push_str("\n\nAssociated Name(s):");
        for name in alternates {
            description.push_str(&format!("\n- {}", text(name)));
        }
    }
    let trackers: Vec<_> =
        labelled(root, &["Track"]).into_iter().flat_map(|li| li.select(&sel("span[data-tip] > a")).collect::<Vec<_>>()).collect();
    if !trackers.is_empty() {
        description.push_str("\n\nTracker(s):");
        for tracker in trackers {
            let name = tracker.parent().and_then(ElementRef::wrap).and_then(|p| attr(p, "data-tip")).unwrap_or_default();
            let href = tracker.value().attr("href").and_then(|h| resolve(at, h)).unwrap_or_default();
            description.push_str(&format!("\n- [{name}]({href})"));
        }
    }

    Ok(SManga {
        author: join(author),
        genre: join(genre),
        status: parse_status(status),
        thumbnail_url: source_img(left, at),
        description: Some(description),
        initialized: true,
        ..SManga::new(url_without_domain(at.as_str()), title)
    })
}

/// "Season 2", "S2": a name with a season restarts its numbering, so the Kotlin numbers it by position.
fn has_season(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.match_indices('s').any(|(i, _)| {
        let rest = &lower[i..];
        let rest = rest.strip_prefix("season").or_else(|| rest.strip_prefix('s')).unwrap_or("");
        rest.trim_start().starts_with(|c: char| c.is_ascii_digit())
    })
}

/// The full chapter list fragment, newest first.
pub fn parse_chapters(html: &str, base: &Url) -> Result<Vec<SChapter>, SourceError> {
    let document = Html::parse_document(html);
    let links: Vec<_> = document.root_element().select(&sel("div[x-data] > a")).collect();
    let total = links.len();
    let img = sel("img");
    links
        .into_iter()
        .enumerate()
        .map(|(index, link)| {
            let name = first(link, "span.flex > span").map(text).ok_or_else(|| SourceError::Parse("a chapter has no name".into()))?;
            let href = link.value().attr("href").and_then(|h| resolve(base, h)).ok_or_else(|| SourceError::Parse("a chapter has no link".into()))?;
            let date = first(link, "time[datetime]")
                .and_then(|t| t.value().attr("datetime"))
                .and_then(|d| chrono::DateTime::parse_from_rfc3339(d).ok())
                .map_or(0, |d| d.timestamp_millis());
            let official = link.select(&img).any(|i| i.value().attr("src").is_some_and(|s| s.to_lowercase().contains("official")));
            let number = if has_season(&name) { (total - index) as f32 } else { -1.0 };
            Ok(SChapter {
                date_upload: date,
                chapter_number: number,
                scanlator: Some(if official { "Official" } else { "Unknown" }.to_owned()),
                ..SChapter::new(url_without_domain(&href), name)
            })
        })
        .collect()
}

/// The image addresses of a chapter's long-strip fragment, in order.
pub fn parse_pages(html: &str, base: &Url) -> Vec<String> {
    let document = Html::parse_document(html);
    // Jsoup's `[x-data~=scroll]` is a regex match, so this is a substring match in CSS.
    document
        .root_element()
        .select(&sel("section[x-data*=scroll] > img"))
        .filter_map(|img| img.value().attr("src").and_then(|src| resolve(base, src)))
        .collect()
}

// ================================= Traits =================================

#[async_trait]
impl MangaSource for WeebCentral {
    fn id(&self) -> i64 {
        self.core.id(NAME, LANG, 1)
    }

    fn name(&self) -> &str {
        NAME
    }

    fn lang(&self) -> &str {
        LANG
    }

    async fn get_manga_details(&self, manga: &SManga) -> Result<SManga, SourceError> {
        self.details(&manga.url).await
    }

    async fn get_chapter_list(&self, manga: &SManga) -> Result<Vec<SChapter>, SourceError> {
        let (html, at) = self.get(&chapters_url(&manga.url)?).await?;
        parse_chapters(&html, &at)
    }

    async fn get_page_list(&self, chapter: &SChapter) -> Result<Vec<Page>, SourceError> {
        let (html, at) = self.get(&pages_url(&chapter.url)?).await?;
        Ok(parse_pages(&html, &at)
            .into_iter()
            .enumerate()
            .map(|(index, image)| Page { image_url: Some(image), ..Page::new(i32::try_from(index).unwrap_or(i32::MAX), "") })
            .collect())
    }
}

#[async_trait]
impl CatalogueSource for WeebCentral {
    fn supports_latest(&self) -> bool {
        true
    }

    fn filter_list(&self) -> FilterList {
        filters_sorted_by("")
    }

    async fn popular(&self, page: i32) -> Result<MangasPage, SourceError> {
        self.list(page, "", &filters_sorted_by("Popularity")).await
    }

    async fn search(&self, page: i32, query: &str, filters: &FilterList) -> Result<MangasPage, SourceError> {
        self.list(page, query, filters).await
    }

    async fn latest(&self, page: i32) -> Result<MangasPage, SourceError> {
        self.list(page, "", &filters_sorted_by("Latest Updates")).await
    }
}

/// The series path a link names (`/series/{id}/{slug}`), as Kotlin `getMangaByUrl`.
pub fn series_path(uri: &str) -> Option<String> {
    let url = Url::parse(uri).ok()?;
    if url.host_str() != Some(HOST) {
        return None;
    }
    let segments: Vec<_> = url.path_segments()?.collect();
    match segments.as_slice() {
        ["series", id, slug, ..] if !id.is_empty() && !slug.is_empty() => Some(format!("/series/{id}/{slug}")),
        _ => None,
    }
}

#[async_trait]
impl ResolvableSource for WeebCentral {
    fn uri_type(&self, uri: &str) -> MangaUriType {
        if series_path(uri).is_some() { MangaUriType::Manga } else { MangaUriType::Unknown }
    }

    async fn manga_from_uri(&self, uri: &str) -> Result<Option<SManga>, SourceError> {
        match series_path(uri) {
            Some(path) => self.details(&path).await.map(Some),
            None => Ok(None),
        }
    }

    async fn chapter_from_uri(&self, _uri: &str) -> Result<Option<SChapter>, SourceError> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn only_the_site_host_over_https_is_fetched() {
        assert!(check_host(&Url::parse("https://weebcentral.com/x").unwrap()).is_ok());
        assert!(check_host(&Url::parse("https://temp.compsci88.com/x").unwrap()).is_err());
        assert!(check_host(&Url::parse("http://weebcentral.com/x").unwrap()).is_err());
    }
}
