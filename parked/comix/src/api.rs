// Ported from keiyoushi src/en/comix (Apache-2.0). Modified for the Kurobayomi plug-in contract; see README.md.
//! Request building and response parsing as pure functions, so the tests run them over saved responses. The JSON of the
//! site (Kotlin `Dto.kt`) is read here and the wire shapes of the app contract are written here.
//!
//! Kotlin preference defaults are built in: large posters, score on top, extra info on, alternative names and narrative
//! tags off, content up to "suggestive". The search filters of the app's list (ABI 1.5) are read by their Kotlin names
//! (`search_params`), and the scanlator blacklist stays at its default (none).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
pub fn encode(text: &str) -> String {
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

// The tables of Kotlin `Filters.kt`. A Select's `state` is an index into its table (Kotlin `vals[state]`).
const SORTABLES: [&str; 11] = [
    "relevance",
    "chapter_updated_at",
    "created_at",
    "title",
    "year",
    "score",
    "views_7d",
    "views_30d",
    "views_90d",
    "views_total",
    "follows_total",
];
/// The content ratings by index: "Show all" (none), then the ratings up to the chosen one.
const RATINGS: [&str; 5] = ["", "safe", "suggestive", "erotica", "pornographic"];
const MATCH_MODES: [&str; 2] = ["and", "or"];
const STATUSES: &[(&str, &str)] = &[
    ("Releasing", "releasing"),
    ("Finished", "finished"),
    ("On Hiatus", "on_hiatus"),
    ("Discontinued", "discontinued"),
    ("Not Yet Released", "not_yet_released"),
];
const TYPES: &[(&str, &str)] = &[("Manga", "manga"), ("Manhwa", "manhwa"), ("Manhua", "manhua"), ("Other", "other")];
const DEMOGRAPHICS: &[(&str, &str)] = &[("Josei", "3"), ("Seinen", "4"), ("Shoujo", "1"), ("Shounen", "2")];
const GENRES: &[(&str, &str)] = &[
    ("Action", "6"),
    ("Adult", "87264"),
    ("Adventure", "7"),
    ("Boys Love", "8"),
    ("Comedy", "9"),
    ("Crime", "10"),
    ("Drama", "11"),
    ("Ecchi", "87265"),
    ("Fantasy", "12"),
    ("Girls Love", "13"),
    ("Harem", "40"),
    ("Hentai", "87266"),
    ("Historical", "14"),
    ("Horror", "15"),
    ("Isekai", "16"),
    ("Magical Girls", "17"),
    ("Mature", "87267"),
    ("Mecha", "18"),
    ("Medical", "19"),
    ("Mystery", "20"),
    ("Philosophical", "21"),
    ("Psychological", "22"),
    ("Romance", "23"),
    ("Sci-Fi", "24"),
    ("Slice of Life", "25"),
    ("Smut", "87268"),
    ("Sports", "26"),
    ("Superhero", "27"),
    ("Thriller", "28"),
    ("Tragedy", "29"),
    ("Wuxia", "30"),
];
const FORMATS: &[(&str, &str)] = &[
    ("4-Koma", "93164"),
    ("Adaptation", "93167"),
    ("Anthology", "93165"),
    ("Award Winning", "93166"),
    ("Doujinshi", "93168"),
    ("Full Color", "93172"),
    ("Long Strip", "93170"),
    ("Oneshot", "93169"),
    ("Web Comic", "93171"),
];

/// The body of a serde-form filter: `{"Select": {...}}` gives `{...}`.
fn filter_body(value: &Value) -> Option<&Value> {
    value.as_object()?.values().next()
}

/// The filter called `name` in the app's FilterList (ABI 1.5), by its body. `None` when the app sent no such filter.
fn filter<'a>(filters: &'a [Value], name: &str) -> Option<&'a Value> {
    filters
        .iter()
        .filter_map(filter_body)
        .find(|body| body.get("name").and_then(Value::as_str) == Some(name))
}

/// The option a Select picked (its index), `None` when the filter is absent.
fn picked_index(filters: &[Value], name: &str) -> Option<usize> {
    usize::try_from(filter(filters, name)?.get("state")?.as_i64()?).ok()
}

/// The display value a Select picked (`values[state]`), `None` when absent.
fn picked_value(filters: &[Value], name: &str) -> Option<String> {
    let body = filter(filters, name)?;
    let index = picked_index(filters, name)?;
    body.get("values")?.as_array()?.get(index)?.as_str().map(str::to_owned)
}

/// The text of a Text filter, empty when absent.
fn text(filters: &[Value], name: &str) -> String {
    filter(filters, name).and_then(|body| body.get("state")?.as_str().map(str::to_owned)).unwrap_or_default()
}

/// The options of a Group filter as `(name, state)`, in the app's order. Empty when absent.
fn group(filters: &[Value], name: &str) -> Vec<(String, Value)> {
    filter(filters, name)
        .and_then(|body| body.get("state")?.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(filter_body)
                .filter_map(|item| Some((item.get("name")?.as_str()?.to_owned(), item.get("state")?.clone())))
                .collect()
        })
        .unwrap_or_default()
}

/// The table id of a label, `None` for a label the Kotlin list does not name.
fn id_of<'a>(table: &[(&str, &'a str)], label: &str) -> Option<&'a str> {
    table.iter().find(|(name, _)| *name == label).map(|(_, id)| *id)
}

/// A CheckBox group as the values of its checked options, in the app's order. With `only_if_some`, a group with every
/// option checked adds nothing (Kotlin `UriMultiSelectFilter`).
fn checked_values(filters: &[Value], name: &str, table: &[(&str, &str)], only_if_some: bool) -> Vec<String> {
    let checked: Vec<String> = group(filters, name)
        .into_iter()
        .filter(|(_, state)| state.as_bool() == Some(true))
        .map(|(label, _)| label)
        .collect();
    if only_if_some && checked.len() == table.len() {
        return Vec::new();
    }
    checked.iter().filter_map(|label| id_of(table, label)).map(str::to_owned).collect()
}

/// A TriState group as `(include, exclude)` ids, in the group's order (Kotlin `TermGroupFilter`).
fn tri_ids(filters: &[Value], name: &str, table: &[(&str, &str)]) -> (Vec<String>, Vec<String>) {
    let (mut include, mut exclude) = (Vec::new(), Vec::new());
    for (label, state) in group(filters, name) {
        let Some(id) = id_of(table, &label) else { continue };
        match state.as_i64() {
            Some(1) => include.push(id.to_owned()),
            Some(2) => exclude.push(id.to_owned()),
            _ => {}
        }
    }
    (include, exclude)
}

/// A sort selection `{"index", "ascending"}`; `None` for a `null` state (no explicit sort).
fn sort_state(body: &Value) -> Option<(usize, bool)> {
    let state = body.get("state")?.as_object()?;
    let index = usize::try_from(state.get("index")?.as_i64()?).ok()?;
    Some((index, state.get("ascending")?.as_bool()?))
}

/// Kotlin `resolveTagIdsForNames`: each comma-separated name, trimmed, empty names dropped, looked up by `tag_ids`.
fn resolve_names(
    tag_ids: &mut impl FnMut(&str, &str) -> Vec<String>,
    kind: &str,
    raw: &str,
) -> Vec<String> {
    let mut ids = Vec::new();
    for name in raw.split(',').map(str::trim).filter(|n| !n.is_empty()) {
        ids.extend(tag_ids(kind, name));
    }
    ids
}

/// The browse parameters of a search: Kotlin `getSearchMangaList` over the app's filter list (ABI 1.5). `tag_ids`
/// looks up the site ids of an author, artist or tag name (`kind` "author", "artist" or "tag"); the plug-in makes that
/// request itself. An absent filter takes its Kotlin default, so an empty list is the default search: the sort is
/// relevance with a keyword and the latest update without one, the ratings are safe and suggestive.
pub fn search_params(
    query: &str,
    filters: &[Value],
    mut tag_ids: impl FnMut(&str, &str) -> Vec<String>,
) -> Result<Params, String> {
    let query = query.trim();
    let authors = resolve_names(&mut tag_ids, "author", &text(filters, "Author"));
    let artists = resolve_names(&mut tag_ids, "artist", &text(filters, "Artist"));
    let tags = resolve_names(&mut tag_ids, "tag", &text(filters, "Tags"));
    let (genre_in, genre_ex) = tri_ids(filters, "Genres", GENRES);
    let (format_in, format_ex) = tri_ids(filters, "Formats", FORMATS);
    let term_selected = !tags.is_empty()
        || [group(filters, "Genres"), group(filters, "Formats")]
            .iter()
            .flatten()
            .any(|(_, state)| state.as_i64().is_some_and(|s| s != 0));

    let mut pairs: Vec<(String, String)> = Vec::new();
    let sort = match filter(filters, "Sort By") {
        None => Some((0, false)),
        Some(body) => sort_state(body),
    };
    if let Some((index, ascending)) = sort {
        let picked = SORTABLES.get(index).copied().unwrap_or("relevance");
        let order = if picked == "relevance" && query.is_empty() { "chapter_updated_at" } else { picked };
        pairs.push((format!("order[{order}]"), if ascending { "asc" } else { "desc" }.to_owned()));
    }
    let rating = picked_index(filters, "Content rating").unwrap_or(2);
    for rating in RATINGS.get(1..=rating).unwrap_or_default() {
        pairs.push(("content_rating[]".to_owned(), (*rating).to_owned()));
    }
    for value in checked_values(filters, "Type", TYPES, true) {
        pairs.push(("types[]".to_owned(), value));
    }
    for (include, exclude) in [(&genre_in, &genre_ex), (&format_in, &format_ex)] {
        for id in include {
            pairs.push(("genres_in[]".to_owned(), id.clone()));
        }
        for id in exclude {
            pairs.push(("genres_ex[]".to_owned(), id.clone()));
        }
    }
    for value in checked_values(filters, "Demographic", DEMOGRAPHICS, true) {
        pairs.push(("demographics[]".to_owned(), value));
    }
    for value in checked_values(filters, "Status", STATUSES, false) {
        pairs.push(("statuses[]".to_owned(), value));
    }
    let min_chapter = text(filters, "Minimum Chapter Length");
    if !min_chapter.is_empty() {
        let value = min_chapter
            .parse::<i32>()
            .ok()
            .filter(|v| *v > 0)
            .ok_or("Minimum chapter length must be a positive integer greater than 0")?;
        pairs.push(("min_chap".to_owned(), value.to_string()));
    }
    for (key, name) in [("year_from", "From"), ("year_to", "To")] {
        if let Some(year) = picked_value(filters, name).filter(|y| !y.is_empty() && y != "Any") {
            pairs.push((key.to_owned(), year));
        }
    }
    if term_selected
        && let Some(mode) = MATCH_MODES.get(picked_index(filters, "Match").unwrap_or(0))
    {
        pairs.push(("genres_mode".to_owned(), (*mode).to_owned()));
    }
    for id in tags {
        pairs.push(("genres_in[]".to_owned(), id));
    }
    for id in authors {
        pairs.push(("authors[]".to_owned(), id));
    }
    for id in artists {
        pairs.push(("artists[]".to_owned(), id));
    }
    if !query.is_empty() {
        pairs.push(("keyword".to_owned(), query.to_owned()));
    }

    let mut out = Params::new();
    for (key, value) in pairs {
        out.entry(key).or_default().push(value);
    }
    Ok(out)
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
    /// The site's relative upload time, such as "5d ago" (Kotlin `createdAtFormatted`).
    #[serde(default)]
    created_at_formatted: String,
}

impl ApiChapter {
    /// `title/{slug}/{id}-chapter-{n}` with no leading slash, as the Kotlin source stores it. `now` is the host clock in
    /// Unix seconds, for the upload date.
    fn to_chapter(&self, manga_slug: &str, now: i64) -> WireChapter {
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
            date_upload: upload_date(&self.created_at_formatted, now),
            chapter_number: self.number as f32,
            scanlator: Some(scanlator),
        }
    }
}

/// Kotlin `parseRelativeDate`: "5d ago" (or "5d") is `now` minus that span, in epoch milliseconds. Units: s, sec(s), m,
/// min(s), h, hr(s), d, day(s), w, week(s), mo, mos, month(s), y, yr(s), year(s); anything else, and an empty text, is 0.
/// `now` is the host clock in Unix seconds. Months and years move the calendar in UTC and clamp the day, as Calendar.add
/// does. ponytail: the other units are fixed spans of UTC seconds, while the Kotlin uses the device zone, which differs
/// only across a daylight-saving change.
pub fn upload_date(text: &str, now: i64) -> i64 {
    let lower = text.trim().to_lowercase();
    let body = lower.strip_suffix(" ago").unwrap_or(lower.as_str());
    let digits = body.bytes().take_while(u8::is_ascii_digit).count();
    let Ok(amount) = body[..digits].parse::<i32>() else { return 0 };
    let amount = i64::from(amount);
    let seconds_per = match body[digits..].trim_start() {
        "s" | "sec" | "secs" => 1,
        "m" | "min" | "mins" => 60,
        "h" | "hr" | "hrs" => 3_600,
        "d" | "day" | "days" => 86_400,
        "w" | "week" | "weeks" => 7 * 86_400,
        "mo" | "mos" | "month" | "months" => return shift_months(now, amount),
        "y" | "yr" | "yrs" | "year" | "years" => return shift_months(now, 12 * amount),
        _ => return 0,
    };
    (now - seconds_per * amount) * 1_000
}

/// `now` moved back `months` calendar months in UTC: the day clamped to the length of the new month, the time of day
/// kept. Result in epoch milliseconds.
fn shift_months(now: i64, months: i64) -> i64 {
    let days = now.div_euclid(86_400);
    let time = now.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let index = year * 12 + i64::from(month) - 1 - months;
    let (year, month) = (index.div_euclid(12), index.rem_euclid(12) as u32 + 1);
    let day = day.min(days_in_month(year, month));
    (days_from_civil(year, month, day) * 86_400 + time) * 1_000
}

/// Days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = i64::from((month + 9) % 12);
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The (year, month, day) of a count of days since 1970-01-01.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
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

/// One page of chapters and whether another follows. `now` is the host clock in Unix seconds.
pub fn chapter_page(
    body: &[u8],
    keys: &Keys,
    manga_slug: &str,
    now: i64,
) -> Result<(Vec<WireChapter>, bool), String> {
    let items: Items<ApiChapter> = decode(body, keys)?;
    Ok((
        items
            .items
            .iter()
            .map(|c| c.to_chapter(manga_slug, now))
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
/// text, so the element ends at the first `</script>` after its start tag. The URL is the detail's own (`url` after
/// `/title`, or `/{hid}`), as the Kotlin `toSManga` takes it, not the address the page was read from.
pub fn details(html: &str) -> Result<WireManga, String> {
    let data = initial_data(html).ok_or("Could not find initial data in page")?;
    let value: serde_json::Value = serde_json::from_str(data).map_err(|e| e.to_string())?;
    let detail = value
        .get("queries")
        .and_then(serde_json::Value::as_object)
        .and_then(|q| q.iter().find(|(k, _)| k.contains("\"detail\"")))
        .map(|(_, v)| v.clone())
        .ok_or("Could not find manga detail in queries")?;
    let manga: ApiManga = serde_json::from_value(detail).map_err(|e| e.to_string())?;
    Ok(manga.to_details())
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

    fn no_ids(_: &str, _: &str) -> Vec<String> {
        Vec::new()
    }

    fn joined(p: &Params) -> String {
        canonical(p).iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("&")
    }

    #[test]
    fn the_search_defaults_are_relevance_with_the_keyword_or_latest_without() {
        assert_eq!(
            joined(&search_params("villains", &[], no_ids).unwrap()),
            "content_rating[0]=safe&content_rating[1]=suggestive&keyword=villains&order[relevance]=desc"
        );
        assert_eq!(
            joined(&search_params(" ", &[], no_ids).unwrap()),
            "content_rating[0]=safe&content_rating[1]=suggestive&order[chapter_updated_at]=desc"
        );
    }

    /// The app's filter list in the serde form, with the states a user picks, and the Kotlin query each one makes.
    #[test]
    fn the_filters_become_the_kotlin_query_parameters() {
        let filters: Vec<Value> = serde_json::from_str(
            r#"[
                {"Sort": {"name": "Sort By", "values": ["Relevance", "Latest", "x", "y", "z", "Highest rated"], "state": {"index": 5, "ascending": false}}},
                {"Select": {"name": "Content rating", "values": ["Show all", "Safe only", "Up to Suggestive", "Up to Erotica", "Up to Pornographic"], "state": 3}},
                {"Group": {"name": "Type", "state": [
                    {"CheckBox": {"name": "Manga", "state": true}}, {"CheckBox": {"name": "Manhwa", "state": false}},
                    {"CheckBox": {"name": "Manhua", "state": true}}, {"CheckBox": {"name": "Other", "state": false}}]}},
                {"Text": {"name": "Tags", "state": "Isekai, Romance"}},
                {"Group": {"name": "Genres", "state": [
                    {"TriState": {"name": "Action", "state": 1}}, {"TriState": {"name": "Comedy", "state": 2}}]}},
                {"Group": {"name": "Formats", "state": [{"TriState": {"name": "Long Strip", "state": 1}}]}},
                {"Select": {"name": "Match", "values": ["All (AND)", "Any (OR)"], "state": 1}},
                {"Group": {"name": "Demographic", "state": [
                    {"CheckBox": {"name": "Josei", "state": true}}, {"CheckBox": {"name": "Seinen", "state": false}},
                    {"CheckBox": {"name": "Shoujo", "state": true}}, {"CheckBox": {"name": "Shounen", "state": true}}]}},
                {"Group": {"name": "Status", "state": [
                    {"CheckBox": {"name": "Releasing", "state": true}}, {"CheckBox": {"name": "Finished", "state": false}},
                    {"CheckBox": {"name": "On Hiatus", "state": false}}, {"CheckBox": {"name": "Discontinued", "state": false}},
                    {"CheckBox": {"name": "Not Yet Released", "state": false}}]}},
                {"Text": {"name": "Minimum Chapter Length", "state": "20"}},
                {"Select": {"name": "From", "values": ["2026", "2025", "Any"], "state": 1}},
                {"Select": {"name": "To", "values": ["Any", "2026", "2025"], "state": 0}},
                {"Text": {"name": "Author", "state": "Oda"}},
                {"Text": {"name": "Artist", "state": ""}}
            ]"#,
        )
        .unwrap();
        let ids = |kind: &str, name: &str| -> Vec<String> {
            match (kind, name) {
                ("tag", "Isekai") => vec!["16".into()],
                ("tag", "Romance") => vec!["23".into()],
                ("author", "Oda") => vec!["77".into()],
                _ => Vec::new(),
            }
        };
        let params = search_params("x", &filters, ids).unwrap();
        assert_eq!(
            joined(&params),
            "authors[0]=77&content_rating[0]=safe&content_rating[1]=suggestive&content_rating[2]=erotica\
             &demographics[0]=3&demographics[1]=1&demographics[2]=2&genres_ex[0]=9\
             &genres_in[0]=6&genres_in[1]=93170&genres_in[2]=16&genres_in[3]=23&genres_mode=or&keyword=x\
             &min_chap=20&order[score]=desc&statuses[0]=releasing&types[0]=manga&types[1]=manhua&year_from=2025"
        );
    }

    #[test]
    fn a_filter_the_list_lacks_takes_its_kotlin_default_and_a_bad_minimum_fails() {
        let only_sort: Vec<Value> = serde_json::from_str(
            r#"[{"Sort": {"name": "Sort By", "values": [], "state": null}}]"#,
        )
        .unwrap();
        assert_eq!(joined(&search_params("", &only_sort, no_ids).unwrap()), "content_rating[0]=safe&content_rating[1]=suggestive");
        let bad: Vec<Value> = serde_json::from_str(r#"[{"Text": {"name": "Minimum Chapter Length", "state": "0"}}]"#).unwrap();
        assert_eq!(
            search_params("", &bad, no_ids).unwrap_err(),
            "Minimum chapter length must be a positive integer greater than 0"
        );
    }

    /// Kotlin `parseRelativeDate` against a clock: the site's relative text, read as an epoch time in milliseconds.
    #[test]
    fn the_upload_dates_are_the_relative_text_read_against_the_clock() {
        let now = days_from_civil(2026, 10, 11) * 86_400 + 3_600;
        let ms = |secs: i64| secs * 1_000;
        assert_eq!(upload_date("5d ago", now), ms(now - 5 * 86_400));
        assert_eq!(upload_date(" 12 mins ago ", now), ms(now - 12 * 60));
        assert_eq!(upload_date("5 hr", now), ms(now - 5 * 3_600));
        assert_eq!(upload_date("2 weeks ago", now), ms(now - 14 * 86_400));
        assert_eq!(upload_date("2 mos ago", now), ms(days_from_civil(2026, 8, 11) * 86_400 + 3_600));
        assert_eq!(upload_date("1 year ago", now), ms(days_from_civil(2025, 10, 11) * 86_400 + 3_600));
        let march_end = days_from_civil(2026, 3, 31) * 86_400;
        assert_eq!(upload_date("1 month ago", march_end), ms(days_from_civil(2026, 2, 28) * 86_400));
        let leap = days_from_civil(2024, 2, 29) * 86_400;
        assert_eq!(upload_date("1 year ago", leap), ms(days_from_civil(2023, 2, 28) * 86_400));
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        for other in ["", "5 hours ago", "x days", "3", "2 fortnights"] {
            assert_eq!(upload_date(other, now), 0, "{other:?}");
        }
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
        let now = 1_000_000_000;
        let out = chapter.to_chapter("x0ynk-villains-are-destined-to-die", now);
        assert_eq!(
            out.url,
            "title/x0ynk-villains-are-destined-to-die/11452041-chapter-214"
        );
        assert_eq!(
            (out.name.as_str(), out.chapter_number, out.scanlator.as_deref()),
            ("Chapter 214", 214.0, Some("Eva Scans"))
        );
        assert_eq!(out.date_upload, (now - 5 * 86_400) * 1_000, "createdAtFormatted \"5d ago\" is five days back");

        let bare: ApiChapter =
            serde_json::from_str(r#"{"id":7,"number":5.5,"name":"Side","isOfficial":true}"#).unwrap();
        let out = bare.to_chapter("abc-slug", now);
        assert_eq!(out.date_upload, 0, "no createdAtFormatted, no date");
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
        let details = details(html).unwrap();
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
        let (chapters, next) = chapter_page(body, &Keys::current().unwrap(), "px0j-androids-have-no-blood", 0).unwrap();
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
