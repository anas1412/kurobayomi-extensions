// Ported from keiyoushi src/en/comix (Apache-2.0). Modified for the Kurobayomi plug-in contract; see README.md.
//! Request building and response parsing as pure functions, so the tests run them over saved responses. The JSON of the
//! site (Kotlin `Dto.kt`) is read here and the wire shapes of the app contract are written here.
//!
//! Kotlin preference defaults are built in: large posters, score on top, extra info on, alternative names and narrative
//! tags off, content up to "suggestive". Tags, author and artist filters and the scanlator blacklist are left out.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::keys::Keys;

/// Query parameters by raw name (`content_rating[]` for a list), as the Kotlin `nativeMangaParams` map.
pub type Params = BTreeMap<String, Vec<String>>;

/// Builds [`Params`] from pairs; a repeated name is one list.
pub fn params(pairs: &[(&str, &str)]) -> Params {
    let mut out = Params::new();
    for (k, v) in pairs {
        out.entry((*k).to_owned()).or_default().push((*v).to_owned());
    }
    out
}

/// Kotlin `canonicalEntries`: names in order, `[]` lists numbered `name[0]`, `name[1]`, a single plain value as is.
pub fn canonical(params: &Params) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (raw, values) in params {
        let name = raw.strip_suffix("[]").unwrap_or(raw);
        if values.len() == 1 && !raw.ends_with("[]") {
            out.push((name.to_owned(), values[0].clone()));
        } else {
            out.extend(
                values
                    .iter()
                    .enumerate()
                    .map(|(i, v)| (format!("{name}[{i}]"), v.clone())),
            );
        }
    }
    out
}

/// Percent-encoding for a query part, as OkHttp writes it: a space is `%20`, and only letters, digits and `*-._` stay.
fn encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The signed address of an API call: `{base}/api/v1{path}?{canonical query}&_={token}`. Values are trimmed first, so
/// the address and the signed text agree.
pub fn signed_url(base: &str, path: &str, params: &Params, keys: &Keys) -> String {
    let entries: Vec<(String, String)> = canonical(params)
        .into_iter()
        .map(|(k, v)| (k, v.trim().to_owned()))
        .collect();
    let text = entries
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");
    let mut query: Vec<String> = entries
        .iter()
        .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
        .collect();
    query.push(format!("_={}", keys.sign(path, &text)));
    format!("{base}/api/v1{path}?{}", query.join("&"))
}

/// The `canonical` text of the browse parameters in search: the default filters with the keyword (Kotlin
/// `getSearchMangaList` with its default filter values). The app sends no filters in ABI 1.4, so these are always the
/// defaults.
pub fn search_params(query: &str) -> Params {
    let query = query.trim();
    let order = if query.is_empty() { "chapter_updated_at" } else { "relevance" };
    let key = format!("order[{order}]");
    let mut out = params(&[(key.as_str(), "desc")]);
    out.insert("content_rating[]".into(), vec!["safe".into(), "suggestive".into()]);
    if !query.is_empty() {
        out.insert("keyword".into(), vec![query.to_owned()]);
    }
    out
}

/// The preference filters that every listing carries (Kotlin `applyPreferenceFilters` with the defaults).
pub fn preference_params(params: &mut Params) {
    params.insert("content_rating[]".into(), vec!["safe".into(), "suggestive".into()]);
}

/// An API body as `T`: the `result` of `{"status":"ok","result":...}`, decrypted first when it is `{"e": ...}`.
pub fn decode<T: for<'de> Deserialize<'de>>(body: &[u8], keys: &Keys) -> Result<T, String> {
    let parse = |e: serde_json::Error| format!("comix json: {e}");
    let value: serde_json::Value = serde_json::from_slice(body).map_err(parse)?;
    let value = match value.get("e").and_then(serde_json::Value::as_str) {
        Some(e) => serde_json::from_slice(&keys.decrypt(e)?).map_err(parse)?,
        None => value,
    };
    serde_json::from_value::<Envelope<T>>(value)
        .map(|e| e.result)
        .map_err(parse)
}

// ---------- the output shapes of the app contract (camelCase) ----------

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireManga {
    pub url: String,
    pub title: String,
    pub artist: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
    pub genre: Option<String>,
    pub status: i32,
    pub thumbnail_url: Option<String>,
    pub initialized: bool,
}

impl WireManga {
    fn basic(url: String, title: &str, thumbnail_url: Option<String>) -> Self {
        Self {
            url,
            title: title.to_owned(),
            artist: None,
            author: None,
            description: None,
            genre: None,
            status: 0,
            thumbnail_url,
            initialized: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireMangasPage {
    pub mangas: Vec<WireManga>,
    pub has_next_page: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireChapter {
    pub url: String,
    pub name: String,
    pub date_upload: i64,
    pub chapter_number: f32,
    pub scanlator: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WirePage {
    pub url: String,
    pub image_url: Option<String>,
}

// ---------- the site's JSON (Kotlin Dto.kt) ----------

#[derive(Deserialize)]
struct Envelope<T> {
    result: T,
}

#[derive(Deserialize)]
struct Term {
    title: String,
}

#[derive(Deserialize, Default)]
struct Poster {
    small: Option<String>,
    medium: Option<String>,
    large: Option<String>,
}

impl Poster {
    fn large(&self) -> Option<String> {
        self.large
            .clone()
            .or_else(|| self.medium.clone())
            .or_else(|| self.small.clone())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiManga {
    hid: String,
    title: String,
    synopsis: Option<String>,
    #[serde(default, rename = "type")]
    kind: String,
    poster: Option<Poster>,
    #[serde(default)]
    status: String,
    #[serde(default = "safe")]
    content_rating: String,
    authors: Option<Vec<Term>>,
    #[serde(rename = "author")]
    author_old: Option<Vec<Term>>,
    artists: Option<Vec<Term>>,
    #[serde(rename = "artist")]
    artist_old: Option<Vec<Term>>,
    genres: Option<Vec<Term>>,
    #[serde(rename = "genre")]
    genre_old: Option<Vec<Term>>,
    demographics: Option<Vec<Term>>,
    #[serde(rename = "demographic")]
    demographic_old: Option<Vec<Term>>,
    #[serde(default)]
    rated_avg: f64,
    #[serde(default)]
    rated_count: i64,
    #[serde(default)]
    follows_total: i64,
    #[serde(default)]
    rank: i64,
    year: Option<i64>,
    original_language: Option<String>,
    links: Option<BTreeMap<String, Option<String>>>,
    url: Option<String>,
}

fn safe() -> String {
    "safe".to_owned()
}

fn titles(terms: &Option<Vec<Term>>, old: &Option<Vec<Term>>) -> Option<Vec<String>> {
    terms
        .as_ref()
        .or(old.as_ref())
        .map(|t| t.iter().map(|t| t.title.clone()).collect())
}

impl ApiManga {
    /// `/x0ynk-villains-are-destined-to-die`: the site URL after `/title`, as the Kotlin source stores it.
    fn url(&self) -> String {
        match &self.url {
            Some(url) => url
                .split_once("/title")
                .map_or(url.as_str(), |(_, rest)| rest)
                .to_owned(),
            None => format!("/{}", self.hid),
        }
    }

    fn to_basic(&self) -> WireManga {
        WireManga::basic(
            self.url(),
            &self.title,
            self.poster.as_ref().and_then(Poster::large),
        )
    }

    fn to_details(&self) -> WireManga {
        let mut out = self.to_basic();
        out.author = titles(&self.authors, &self.author_old).map(|t| t.join(", "));
        out.artist = titles(&self.artists, &self.artist_old).map(|t| t.join(", "));
        out.description = Some(self.description());
        out.status = match self.status.as_str() {
            "releasing" => 1,
            "on_hiatus" => 6,
            "finished" => 2,
            "discontinued" => 5,
            _ => 0,
        };
        out.genre = Some(self.genres());
        out.initialized = true;
        out
    }

    fn fancy_score(&self) -> String {
        if self.rated_avg == 0.0 {
            return String::new();
        }
        let stars = ((self.rated_avg / 2.0) + 0.5).floor().clamp(0.0, 5.0) as usize;
        format!(
            "{}{} {}",
            "★".repeat(stars),
            "☆".repeat(5 - stars),
            self.rated_avg
        )
    }

    fn description(&self) -> String {
        let mut out = String::new();
        let score = self.fancy_score();
        if !score.is_empty() {
            out.push_str(&score);
            out.push_str("\n\n");
        }
        out.push_str(self.synopsis.as_deref().unwrap_or(""));
        let mut extras = Vec::new();
        if let Some(year) = self.year.filter(|y| *y > 0) {
            extras.push(format!("**Year**: {year}"));
        }
        if let Some(lang) = self
            .original_language
            .as_deref()
            .filter(|l| !l.trim().is_empty())
        {
            extras.push(format!("**Language**: {}", lang.to_uppercase()));
        }
        if let Some(first) = self.content_rating.chars().next() {
            extras.push(format!(
                "**Content rating**: {}{}",
                first.to_uppercase(),
                &self.content_rating[first.len_utf8()..]
            ));
        }
        if self.rank > 0 {
            extras.push(format!("**Rank**: #{}", self.rank));
        }
        if self.rated_count > 0 {
            extras.push(format!("**Rated by**: {}", self.rated_count));
        }
        if self.follows_total > 0 {
            extras.push(format!("**Followed by**: {}", self.follows_total));
        }
        if !extras.is_empty() {
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            out.push_str(&extras.join("\n"));
        }
        let trackers = self.tracker_links();
        if !trackers.is_empty() {
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            out.push_str("**Trackers**:\n");
            out.push_str(
                &trackers
                    .iter()
                    .map(|t| format!("- {t}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
        }
        out
    }

    fn tracker_links(&self) -> Vec<String> {
        const SITES: [(&str, &str); 5] = [
            ("al", "AniList"),
            ("mal", "MyAnimeList"),
            ("mu", "MangaUpdates"),
            ("md", "MangaDex"),
            ("mb", "MangaBaka"),
        ];
        let Some(links) = &self.links else {
            return Vec::new();
        };
        let known = SITES.iter().filter_map(|(key, name)| {
            links
                .get(*key)?
                .as_deref()
                .filter(|u| !u.trim().is_empty())
                .map(|u| format!("[{name}]({u})"))
        });
        let unknown = links
            .iter()
            .filter(|(k, _)| !SITES.iter().any(|(s, _)| s == k))
            .filter_map(|(k, u)| {
                u.as_deref()
                    .filter(|u| u.starts_with("http"))
                    .map(|u| format!("[{}]({u})", k.to_uppercase()))
            });
        known.chain(unknown).collect()
    }

    fn genres(&self) -> String {
        let mut out = vec![
            match self.kind.as_str() {
                "manhwa" => "Manhwa",
                "manhua" => "Manhua",
                "manga" => "Manga",
                _ => "Other",
            }
            .to_owned(),
        ];
        out.extend(titles(&self.genres, &self.genre_old).unwrap_or_default());
        out.extend(titles(&self.demographics, &self.demographic_old).unwrap_or_default());
        if matches!(self.content_rating.as_str(), "erotica" | "pornographic") {
            out.push("NSFW".to_owned());
        }
        let mut seen = std::collections::HashSet::new();
        out.retain(|g| seen.insert(g.clone()));
        out.join(", ")
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Meta {
    #[serde(default = "one")]
    page: i64,
    #[serde(default = "one")]
    last_page: i64,
    #[serde(default = "one", rename = "last_page")]
    last_page_old: i64,
}

fn one() -> i64 {
    1
}

#[derive(Deserialize)]
struct Items<T> {
    #[serde(default = "Vec::new")]
    items: Vec<T>,
    meta: Option<Meta>,
    pagination: Option<Meta>,
}

impl<T> Items<T> {
    fn has_next_page(&self) -> bool {
        self.meta
            .as_ref()
            .or(self.pagination.as_ref())
            .is_some_and(|m| m.page < m.last_page.max(m.last_page_old))
    }
}

#[derive(Deserialize)]
struct Group {
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiChapter {
    id: i64,
    #[serde(default)]
    url: String,
    number: f64,
    #[serde(default)]
    name: String,
    group: Option<Group>,
    #[serde(default)]
    is_official: bool,
}

impl ApiChapter {
    /// `title/{slug}/{id}-chapter-{n}` with no leading slash, as the Kotlin source stores it. The upload date is 0: the
    /// plug-in has no clock (see README).
    fn to_chapter(&self, manga_slug: &str) -> WireChapter {
        let number = format!("{}", self.number);
        let url = match self.url.find("/title/") {
            Some(i) => self.url[i + 1..].to_owned(),
            None => format!("title/{manga_slug}/{}-chapter-{number}", self.id),
        };
        let name = if self.name.is_empty() {
            format!("Chapter {number}")
        } else {
            format!("Chapter {number}: {}", self.name)
        };
        let scanlator = match (&self.group, self.is_official) {
            (Some(group), _) => group.name.clone(),
            (None, true) => "Official".to_owned(),
            (None, false) => "Unknown".to_owned(),
        };
        WireChapter {
            url,
            name,
            date_upload: 0,
            chapter_number: self.number as f32,
            scanlator: Some(scanlator),
        }
    }
}

#[derive(Deserialize)]
struct PageItem {
    url: String,
    #[serde(default)]
    s: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pages {
    #[serde(default)]
    base_url: String,
    items: Vec<PageItem>,
    alt: Option<Vec<String>>,
    node: Option<String>,
}

impl Pages {
    /// The hosts the site's reader retries a page on, in its order (`Dto.kt` `altHosts`).
    fn alt_hosts(&self, host: &str, index: usize) -> Vec<String> {
        let Some(alt) = self.alt.as_ref().filter(|a| !a.is_empty()) else {
            return Vec::new();
        };
        let label = host.split('.').next().unwrap_or(host);
        let start = index % alt.len();
        let domains: Vec<&String> = alt[start..].iter().chain(&alt[..start]).collect();
        let node = self.node.as_deref().unwrap_or(label);
        let hosts: Vec<String> = domains
            .iter()
            .map(|d| format!("{label}.{d}"))
            .chain(domains.iter().map(|d| format!("{node}.{d}")))
            .collect();
        (0..6).map(|i| hosts[i % hosts.len()].clone()).collect()
    }
}

#[derive(Deserialize)]
struct ChapterResult {
    pages: Pages,
}

// ---------- parsers ----------

pub fn manga_page(body: &[u8], keys: &Keys) -> Result<WireMangasPage, String> {
    let items: Items<ApiManga> = decode(body, keys)?;
    Ok(WireMangasPage {
        mangas: items.items.iter().map(ApiManga::to_basic).collect(),
        has_next_page: items.has_next_page(),
    })
}

/// One page of chapters and whether another follows.
pub fn chapter_page(
    body: &[u8],
    keys: &Keys,
    manga_slug: &str,
) -> Result<(Vec<WireChapter>, bool), String> {
    let items: Items<ApiChapter> = decode(body, keys)?;
    Ok((
        items
            .items
            .iter()
            .map(|c| c.to_chapter(manga_slug))
            .collect(),
        items.has_next_page(),
    ))
}

/// Kotlin `selectChapters` with deduplication and the scanlator blacklist off (their defaults): one per URL, highest
/// number first.
pub fn select_chapters(mut chapters: Vec<WireChapter>) -> Vec<WireChapter> {
    let mut seen = std::collections::HashSet::new();
    chapters.retain(|c| seen.insert(c.url.clone()));
    chapters.sort_by(|a, b| b.chapter_number.total_cmp(&a.chapter_number));
    chapters
}

/// The manga detail inside a title page: the JSON of `<script id="initial-data">`, its `detail` query. The script is raw
/// text, so the element ends at the first `</script>` after its start tag.
pub fn details(html: &str, url: &str) -> Result<WireManga, String> {
    let data = initial_data(html).ok_or("Could not find initial data in page")?;
    let value: serde_json::Value = serde_json::from_str(data).map_err(|e| e.to_string())?;
    let detail = value
        .get("queries")
        .and_then(serde_json::Value::as_object)
        .and_then(|q| q.iter().find(|(k, _)| k.contains("\"detail\"")))
        .map(|(_, v)| v.clone())
        .ok_or("Could not find manga detail in queries")?;
    let manga: ApiManga = serde_json::from_value(detail).map_err(|e| e.to_string())?;
    let mut out = manga.to_details();
    out.url = url.to_owned();
    Ok(out)
}

fn initial_data(html: &str) -> Option<&str> {
    let at = html.find("id=\"initial-data\"")?;
    let open = at + html[at..].find('>')? + 1;
    let close = open + html[open..].find("</script>")?;
    Some(&html[open..close])
}

/// Kotlin `buildPages`: absolute image URLs, the `v3` flag for grid-scrambled pages, and the retry hosts in the
/// fragment `#alt=host1,host2,...` (the fragment never reaches the server; the app must read it, see `docs/`).
pub fn pages(body: &[u8], keys: &Keys) -> Result<Vec<WirePage>, String> {
    let result: ChapterResult = decode(body, keys)?;
    let pages = result.pages;
    let base = pages.base_url.trim_end_matches('/');
    let mut out = Vec::new();
    for (index, item) in pages.items.iter().enumerate() {
        let mut full = if item.url.starts_with("http") {
            item.url.clone()
        } else {
            format!("{base}/{}", item.url.trim_start_matches('/'))
        };
        if item.s == 1 && !has_query_key(&full, "v3") {
            full.push_str(if full.contains('?') { "&v3" } else { "?v3" });
        }
        let alt = pages.alt_hosts(host_of(&full), index);
        if !alt.is_empty() {
            full.push_str(&format!("#alt={}", alt.join(",")));
        }
        out.push(WirePage {
            url: String::new(),
            image_url: Some(full),
        });
    }
    Ok(out)
}

fn has_query_key(url: &str, key: &str) -> bool {
    url.split_once('?').is_some_and(|(_, query)| {
        query
            .split('#')
            .next()
            .unwrap_or("")
            .split('&')
            .any(|pair| pair.split('=').next() == Some(key))
    })
}

/// The host of an absolute http(s) URL: no scheme, port, path, query or fragment.
fn host_of(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    rest[..end].split(':').next().unwrap_or("")
}

/// The id in a stored manga URL: `/x0ynk-villains-...` gives `x0ynk`.
pub fn manga_id(manga_url: &str) -> Option<&str> {
    let slug = manga_slug(manga_url);
    slug.split('-').next().filter(|id| !id.is_empty())
}

/// `x0ynk-villains-...` from `/x0ynk-villains-...`.
pub fn manga_slug(manga_url: &str) -> &str {
    manga_url
        .trim_start_matches('/')
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
}

/// The id in a stored chapter URL: `title/x0ynk-.../11452041-chapter-214` gives `11452041`.
pub fn chapter_id(chapter_url: &str) -> Option<i64> {
    let last = chapter_url.trim_end_matches('/').rsplit('/').next()?;
    last.split("-chapter-").next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_canonical_query_numbers_lists_and_sorts_names() {
        let p = params(&[
            ("page", "1"),
            ("order[score]", "desc"),
            ("content_rating[]", "safe"),
            ("content_rating[]", "suggestive"),
            ("limit", "28"),
        ]);
        let text: Vec<String> = canonical(&p)
            .into_iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        assert_eq!(
            text.join("&"),
            "content_rating[0]=safe&content_rating[1]=suggestive&limit=28&order[score]=desc&page=1"
        );
    }

    /// The canonical text for the browse call matches what the site's signer was given in a browser.
    #[test]
    fn a_signed_url_carries_the_encoded_query_and_the_token() {
        let keys = Keys::current().unwrap();
        let p = params(&[("limit", "28"), ("page", "1")]);
        assert_eq!(
            signed_url("https://comix.to", "/manga", &p, &keys),
            "https://comix.to/api/v1/manga?limit=28&page=1&_=17w.gZzX1AqVvudWzvBR"
        );
        let p = params(&[("keyword", " naruto & co/é+ "), ("page", "1")]);
        assert_eq!(
            signed_url("https://comix.to", "/manga", &p, &keys),
            "https://comix.to/api/v1/manga?keyword=naruto%20%26%20co%2F%C3%A9%2B&page=1&_=17w.9Cq8M827sGNHQIjG"
        );
    }

    #[test]
    fn the_search_defaults_are_relevance_with_the_keyword_or_latest_without() {
        assert_eq!(
            canonical(&search_params("villains")).iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("&"),
            "content_rating[0]=safe&content_rating[1]=suggestive&keyword=villains&order[relevance]=desc"
        );
        assert_eq!(
            canonical(&search_params(" ")).iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("&"),
            "content_rating[0]=safe&content_rating[1]=suggestive&order[chapter_updated_at]=desc"
        );
    }

    #[test]
    fn ids_come_from_the_stored_urls() {
        assert_eq!(
            manga_id("/x0ynk-villains-are-destined-to-die"),
            Some("x0ynk")
        );
        assert_eq!(
            manga_slug("/x0ynk-villains-are-destined-to-die"),
            "x0ynk-villains-are-destined-to-die"
        );
        assert_eq!(manga_id("/"), None);
        assert_eq!(
            chapter_id("title/x0ynk-villains-are-destined-to-die/11452041-chapter-214"),
            Some(11452041)
        );
        assert_eq!(chapter_id("title/x/abc"), None);
    }

    #[test]
    fn chapters_are_unique_by_url_and_highest_first() {
        let c = |url: &str, n: f32| WireChapter {
            url: url.to_owned(),
            name: url.to_owned(),
            date_upload: 0,
            chapter_number: n,
            scanlator: None,
        };
        let out = select_chapters(vec![c("a", 1.0), c("b", 3.0), c("a", 1.0), c("c", 2.5)]);
        assert_eq!(
            out.iter().map(|c| c.url.as_str()).collect::<Vec<_>>(),
            ["b", "c", "a"]
        );
    }

    /// The first chapter item of the real decrypted response for `x0ynk` (seen in a browser on 2026-10-10).
    #[test]
    fn a_chapter_keeps_the_kotlin_url_name_and_group() {
        let item = r#"{"id":11452041,"mangaId":17667,"number":214,"volume":0,"name":"","language":"en","isOfficial":false,"votes":5,"createdAtFormatted":"5d ago","groupId":9316,"group":{"id":9316,"name":"Eva Scans"},"creator":null,"url":"/title/x0ynk-villains-are-destined-to-die/11452041-chapter-214"}"#;
        let chapter: ApiChapter = serde_json::from_str(item).unwrap();
        let out = chapter.to_chapter("x0ynk-villains-are-destined-to-die");
        assert_eq!(
            out.url,
            "title/x0ynk-villains-are-destined-to-die/11452041-chapter-214"
        );
        assert_eq!(
            (out.name.as_str(), out.chapter_number, out.scanlator.as_deref()),
            ("Chapter 214", 214.0, Some("Eva Scans"))
        );

        let bare: ApiChapter =
            serde_json::from_str(r#"{"id":7,"number":5.5,"name":"Side","isOfficial":true}"#).unwrap();
        let out = bare.to_chapter("abc-slug");
        assert_eq!(
            (out.url.as_str(), out.name.as_str(), out.scanlator.as_deref()),
            ("title/abc-slug/7-chapter-5.5", "Chapter 5.5: Side", Some("Official"))
        );
    }

    #[test]
    fn alt_hosts_follow_the_reader_order() {
        let pages: Pages =
            serde_json::from_str(r#"{"alt":["a.site","b.site","c.site"],"node":"lov","items":[]}"#)
                .unwrap();
        assert_eq!(
            pages.alt_hosts("8pl.x.site", 1),
            [
                "8pl.b.site",
                "8pl.c.site",
                "8pl.a.site",
                "lov.b.site",
                "lov.c.site",
                "lov.a.site"
            ]
        );
        let none: Pages = serde_json::from_str(r#"{"items":[]}"#).unwrap();
        assert!(none.alt_hosts("8pl.x.site", 0).is_empty());
    }

    #[test]
    fn a_listing_gives_mangas_with_kotlin_urls_and_large_posters() {
        let page = manga_page(include_bytes!("../fixtures/popular.json"), &Keys::current().unwrap()).unwrap();
        assert_eq!(page.mangas.len(), 28);
        assert!(page.has_next_page, "page 1 of 3342");
        let first = &page.mangas[0];
        assert_eq!(
            (first.url.as_str(), first.title.as_str()),
            ("/px0j-androids-have-no-blood", "Androids Have No Blood")
        );
        assert_eq!(
            first.thumbnail_url.as_deref(),
            Some("https://static.comix.to/29c9/i/f/4b/68de559c9da52.jpg")
        );
        assert!(page.mangas.iter().all(|m| m.url.starts_with('/') && !m.title.is_empty()));
    }

    #[test]
    fn the_title_page_gives_the_details() {
        let html = std::str::from_utf8(include_bytes!("../fixtures/title.html")).unwrap();
        let details = details(html, "/x0ynk-villains-are-destined-to-die").unwrap();
        assert_eq!(details.title, "Villains Are Destined to Die");
        assert_eq!(details.url, "/x0ynk-villains-are-destined-to-die");
        assert!(details.initialized);
        assert_ne!(details.status, 0);
        let genre = details.genre.as_deref().unwrap();
        assert!(genre.starts_with("Manhwa"), "{genre}");
        let description = details.description.as_deref().unwrap();
        assert!(description.contains("**Content rating**"), "{description}");
        assert!(details.thumbnail_url.as_deref().is_some_and(|u| u.starts_with("https://")));
    }

    #[test]
    fn an_encrypted_chapter_list_with_no_items_is_empty() {
        let body = include_bytes!("../fixtures/chapters_encrypted.json");
        let (chapters, next) = chapter_page(body, &Keys::current().unwrap(), "px0j-androids-have-no-blood").unwrap();
        assert!(chapters.is_empty() && !next);
    }

    /// Image entries copied from the real decrypted page list of chapter 11452041 (browser, 2026-10-10); the
    /// surrounding object is cut down to the fields the parser reads.
    #[test]
    fn pages_are_absolute_with_retry_hosts() {
        let body = br#"{"status":"ok","result":{"id":11452041,"pages":{"alt":["trueglow85.site","deepforce74.site","wildlife97.site","brightlife09.site","boldstep66.site"],"node":"lov","items":[{"width":720,"height":1408,"url":"https://8pl.markscorner.site/hi/bEqPbYfoNT0Gm1XlBiKfoD5Y4rkdeu6i3R0VvpLI6y4AiVMhaGDNl"},{"width":720,"height":1408,"url":"https://8pl.realtalkjournal.site/hi/bEqPbYfoNT0Gm1XlBiKfoD5Y4rkdeu6i3R0VvpLI6y4AiV8haGDNl"}]}}}"#;
        let out = pages(body, &Keys::current().unwrap()).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(
            out[1].image_url.as_deref(),
            Some(
                "https://8pl.realtalkjournal.site/hi/bEqPbYfoNT0Gm1XlBiKfoD5Y4rkdeu6i3R0VvpLI6y4AiV8haGDNl#alt=8pl.deepforce74.site,8pl.wildlife97.site,8pl.brightlife09.site,8pl.boldstep66.site,8pl.trueglow85.site,lov.deepforce74.site"
            )
        );
        let v3 = br#"{"result":{"pages":{"baseUrl":"https://img.example.site/","items":[{"url":"/p/1.jpg","s":1},{"url":"/p/2.jpg?x=1","s":1}]}}}"#;
        let out = pages(v3, &Keys::current().unwrap()).unwrap();
        assert_eq!(out[0].image_url.as_deref(), Some("https://img.example.site/p/1.jpg?v3"));
        assert_eq!(out[1].image_url.as_deref(), Some("https://img.example.site/p/2.jpg?x=1&v3"));
    }
}
