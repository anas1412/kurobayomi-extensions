//! The MangaDex API (<https://api.mangadex.org>) as pure functions: URLs and one parser per answer. Ported from the
//! keiyoushi `all/mangadex` extension (Apache-2.0). Fixed at the Kotlin defaults for English: safe and suggestive content,
//! every original language, the full-quality images, the extension-language title first.

use serde::Serialize;
use serde_json::Value;

pub type Res<T> = Result<T, String>;

pub const API: &str = "https://api.mangadex.org";
/// Where covers live (`<CDN>/covers/<manga id>/<file>`): loaded by the app.
const CDN: &str = "https://uploads.mangadex.org";
pub const LANG: &str = "en";
pub const MANGA_LIMIT: i32 = 20;
pub const LATEST_LIMIT: i32 = 100;
/// The most the chapter feed gives at once.
pub const FEED_LIMIT: i64 = 500;

// ============================ Output (wire) ============================

#[derive(Debug, Clone, PartialEq, Serialize)]
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
    /// Epoch milliseconds of `publishAt`.
    pub date_upload: i64,
    pub chapter_number: f32,
    pub scanlator: String,
}

/// One page of the chapter feed: its chapters and the offset of the next page, when there is one.
#[derive(Debug, Clone, PartialEq)]
pub struct FeedPage {
    pub chapters: Vec<Chapter>,
    pub next_offset: Option<i64>,
}

// ================================ URLs =================================

fn encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The content the Kotlin shows by default.
const RATINGS: &str = "&contentRating[]=safe&contentRating[]=suggestive";

fn offset(page: i32, limit: i32) -> i32 {
    (page.max(1) - 1) * limit
}

/// Popular: the most followed titles with English chapters that were created in the last 30 days (`since` is that day,
/// `yyyy-MM-ddTHH:mm:ss`).
pub fn popular_url(page: i32, since: &str) -> String {
    format!(
        "{API}/manga?order[followedCount]=desc&availableTranslatedLanguage[]={LANG}&limit={MANGA_LIMIT}&offset={}&includes[]=cover_art{RATINGS}&createdAtSince={}",
        offset(page, MANGA_LIMIT),
        encode(since)
    )
}

/// The newest English chapters; their titles are read with [`by_ids_url`].
pub fn latest_url(page: i32) -> String {
    format!(
        "{API}/chapter?offset={}&limit={LATEST_LIMIT}&translatedLanguage[]={LANG}&order[publishAt]=desc&includeFutureUpdates=0&includeFuturePublishAt=0&includeEmptyPages=0{RATINGS}",
        offset(page, LATEST_LIMIT)
    )
}

pub fn by_ids_url(ids: &[String]) -> String {
    let ids: String = ids.iter().map(|id| format!("&ids[]={}", encode(id))).collect();
    format!("{API}/manga?limit={LATEST_LIMIT}&includes[]=cover_art{RATINGS}{ids}")
}

/// A search by title. Without text the most followed titles show, as the Kotlin does.
pub fn search_url(page: i32, query: &str) -> String {
    let order = if query.trim().is_empty() { "&order[followedCount]=desc" } else { "" };
    let title = if query.trim().is_empty() { String::new() } else { format!("&title={}", encode(query.trim())) };
    format!(
        "{API}/manga?limit={MANGA_LIMIT}&offset={}&availableTranslatedLanguage[]={LANG}&includes[]=cover_art{RATINGS}{title}{order}",
        offset(page, MANGA_LIMIT)
    )
}

pub fn details_url(manga_url: &str) -> Res<String> {
    Ok(format!("{API}/manga/{}?includes[]=cover_art&includes[]=author&includes[]=artist", id_of(manga_url, "manga")?))
}

pub fn feed_url(manga_url: &str, offset: i64) -> Res<String> {
    Ok(format!(
        "{API}/manga/{}/feed?includes[]=scanlation_group&includes[]=user&limit={FEED_LIMIT}&offset={offset}&translatedLanguage[]={LANG}&order[volume]=desc&order[chapter]=desc&includeFuturePublishAt=0&includeEmptyPages=0{RATINGS}",
        id_of(manga_url, "manga")?
    ))
}

pub fn at_home_url(chapter_url: &str) -> Res<String> {
    Ok(format!("{API}/at-home/server/{}", id_of(chapter_url, "chapter")?))
}

/// The id in `/manga/<id>` or `/chapter/<id>`.
fn id_of<'a>(url: &'a str, kind: &str) -> Res<&'a str> {
    let id = url.strip_prefix(&format!("/{kind}/")).unwrap_or(url).split(['/', '?']).next().unwrap_or_default();
    let ok = id.len() == 36 && id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-');
    if ok { Ok(id) } else { Err(format!("not a MangaDex {kind} link: {url}")) }
}

// =============================== Dates =================================

/// Days since 1970-01-01 to `(year, month, day)` (Howard Hinnant's algorithm).
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

/// The start of the day 30 days before `now` (Unix seconds), as `yyyy-MM-ddT00:00:00`.
pub fn thirty_days_before(now: u64) -> String {
    let (y, m, d) = civil((now / 86_400) as i64 - 30);
    format!("{y:04}-{m:02}-{d:02}T00:00:00")
}

/// An ISO 8601 time (`2023-09-05T17:14:12+00:00`) as epoch milliseconds; 0 when it does not read. The offset is applied.
pub fn millis(text: &str) -> i64 {
    let parse = || -> Option<i64> {
        let (date, rest) = text.split_once('T')?;
        let d: Vec<i64> = date.split('-').map(|p| p.parse().ok()).collect::<Option<_>>()?;
        let [y, m, day] = d.as_slice().try_into().ok()?;
        let time = rest.get(..8)?;
        let t: Vec<i64> = time.split(':').map(|p| p.parse().ok()).collect::<Option<_>>()?;
        let [hh, mm, ss] = t.as_slice().try_into().ok()?;
        let tz = &rest[8..];
        let offset = match tz.as_bytes().first() {
            Some(b'+' | b'-') => {
                let sign = if tz.starts_with('-') { -1 } else { 1 };
                let (h, m) = tz[1..].split_once(':')?;
                sign * (h.parse::<i64>().ok()? * 3600 + m.parse::<i64>().ok()? * 60)
            }
            _ => 0,
        };
        let (y, m) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let doy = (153 * m + 2) / 5 + day - 1;
        let days = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
        Some(((days * 24 + hh) * 3600 + mm * 60 + ss - offset) * 1000)
    };
    parse().unwrap_or(0)
}

// =============================== Parsers ===============================

/// The message of an error answer (`{"result":"error","errors":[{"detail":...}]}`); `None` for any other body.
pub fn error_message(body: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body).ok()?;
    (value["result"] == "error").then(|| {
        let detail = value["errors"][0]["detail"].as_str().or(value["errors"][0]["title"].as_str()).unwrap_or("an error");
        format!("MangaDex answered: {detail}")
    })
}

fn decode(body: &str) -> Res<Value> {
    if let Some(message) = error_message(body) {
        return Err(message);
    }
    serde_json::from_str(body).map_err(|e| format!("unexpected answer: {e}"))
}

fn text(value: &Value) -> Option<&str> {
    value.as_str().filter(|s| !s.trim().is_empty())
}

/// The title the Kotlin shows for the extension language: the title in that language, else an alternative title in it,
/// else the main title, else the English one.
fn title_of(attributes: &Value) -> String {
    let in_lang = |lang: &str| attributes["altTitles"].as_array().and_then(|alts| alts.iter().find_map(|alt| text(&alt[lang])));
    text(&attributes["title"][LANG])
        .or_else(|| in_lang(LANG))
        .or_else(|| attributes["title"].as_object().and_then(|t| t.values().find_map(text)))
        .unwrap_or_default()
        .to_owned()
}

/// A cover address from a manga's `cover_art` relationship.
fn cover_of(manga: &Value) -> Option<String> {
    let id = manga["id"].as_str()?;
    let file = manga["relationships"].as_array()?.iter().find(|r| r["type"] == "cover_art")?["attributes"]["fileName"].as_str()?;
    Some(format!("{CDN}/covers/{id}/{file}"))
}

fn basic(manga: &Value) -> Manga {
    Manga {
        url: format!("/manga/{}", manga["id"].as_str().unwrap_or_default()),
        title: title_of(&manga["attributes"]),
        artist: None,
        author: None,
        description: None,
        genre: None,
        status: 0,
        thumbnail_url: cover_of(manga),
    }
}

/// A page of manga (`/manga` answers): the manga, and whether more follow.
pub fn parse_mangas(body: &str) -> Res<MangasPage> {
    let page = decode(body)?;
    let mangas: Vec<Manga> = page["data"].as_array().map(|list| list.iter().map(basic).collect()).unwrap_or_default();
    let (limit, offset, total) = (page["limit"].as_i64().unwrap_or(0), page["offset"].as_i64().unwrap_or(0), page["total"].as_i64().unwrap_or(0));
    Ok(MangasPage { mangas, has_next_page: offset + limit < total })
}

/// The manga ids of a latest-chapters page, newest first, each once, and whether more pages follow.
pub fn parse_latest_ids(body: &str) -> Res<(Vec<String>, bool)> {
    let page = decode(body)?;
    let mut ids: Vec<String> = Vec::new();
    for chapter in page["data"].as_array().into_iter().flatten() {
        let manga = chapter["relationships"].as_array().into_iter().flatten().find(|r| r["type"] == "manga");
        if let Some(id) = manga.and_then(|m| m["id"].as_str()).filter(|id| !ids.iter().any(|seen| seen == id)) {
            ids.push(id.to_owned());
        }
    }
    let (limit, offset, total) = (page["limit"].as_i64().unwrap_or(0), page["offset"].as_i64().unwrap_or(0), page["total"].as_i64().unwrap_or(0));
    Ok((ids, offset + limit < total))
}

/// Puts the manga of a by-ids answer in the order of `ids` (the API does not keep it).
pub fn parse_in_order(body: &str, ids: &[String], has_next_page: bool) -> Res<MangasPage> {
    let page = decode(body)?;
    let list = page["data"].as_array().cloned().unwrap_or_default();
    let mangas = ids.iter().filter_map(|id| list.iter().find(|m| m["id"] == id.as_str())).map(basic).collect();
    Ok(MangasPage { mangas, has_next_page })
}

fn status_of(status: &str) -> i32 {
    match status {
        "ongoing" => 1,
        "completed" => 2,
        "cancelled" => 5,
        "hiatus" => 6,
        _ => 0,
    }
}

fn related_names(manga: &Value, kind: &str) -> Option<String> {
    let mut names: Vec<&str> = Vec::new();
    for r in manga["relationships"].as_array().into_iter().flatten().filter(|r| r["type"] == kind) {
        if let Some(name) = r["attributes"]["name"].as_str().filter(|n| !names.contains(n)) {
            names.push(name);
        }
    }
    Some(names.join(", ")).filter(|n| !n.is_empty())
}

/// One manga with everything: description, genres (tags, then demographic and content rating), people and status.
pub fn parse_details(body: &str) -> Res<Manga> {
    let page = decode(body)?;
    let manga = &page["data"];
    let attributes = &manga["attributes"];
    let mut found = basic(manga);
    found.description = text(&attributes["description"][LANG]).or_else(|| attributes["description"].as_object().and_then(|d| d.values().find_map(text))).map(str::to_owned);
    found.status = status_of(attributes["status"].as_str().unwrap_or_default());
    found.author = related_names(manga, "author");
    found.artist = related_names(manga, "artist");
    let mut genres: Vec<String> = attributes["tags"].as_array().into_iter().flatten().filter_map(|t| t["attributes"]["name"][LANG].as_str()).map(str::to_owned).collect();
    genres.sort();
    if let Some(demographic) = attributes["publicationDemographic"].as_str() {
        genres.push(capitalize(demographic));
    }
    if let Some(rating) = attributes["contentRating"].as_str().filter(|r| *r != "safe") {
        genres.push(format!("Content rating: {}", capitalize(rating)));
    }
    found.genre = Some(genres.join(", ")).filter(|g| !g.is_empty());
    Ok(found)
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map(|c| c.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default()
}

/// Whether a chapter cannot be read in the app: it links to another site and has no pages here (or only the notice that
/// the chapter was removed).
fn is_invalid(attributes: &Value) -> bool {
    !attributes["externalUrl"].is_null() && (attributes["pages"] == 0 || (attributes["pages"] == 1 && attributes["updatedAt"] != attributes["readableAt"]))
}

fn chapter_of(chapter: &Value) -> Chapter {
    let a = &chapter["attributes"];
    let groups: Vec<&str> = chapter["relationships"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| r["type"] == "scanlation_group" && r["id"] != "00e03853-1b96-4f41-9542-c71b8692033b")
        .filter_map(|r| r["attributes"]["name"].as_str())
        .collect();
    let mut scanlator = groups.join(" & ");
    if scanlator.is_empty() {
        let users: Vec<&str> = chapter["relationships"].as_array().into_iter().flatten().filter(|r| r["type"] == "user").filter_map(|r| r["attributes"]["username"].as_str()).collect();
        scanlator = if users.is_empty() { "No Group".to_owned() } else { format!("Uploaded by {}", users.join(" & ")) };
    }
    if a["isUnavailable"] == true {
        scanlator = format!("Unavailable - {scanlator}");
    }
    let mut name: Vec<String> = Vec::new();
    if let Some(volume) = text(&a["volume"]) {
        name.push(format!("Vol.{volume}"));
    }
    if let Some(number) = text(&a["chapter"]) {
        name.push(format!("Ch.{number}"));
    }
    if let Some(title) = text(&a["title"]) {
        if !name.is_empty() {
            name.push("-".to_owned());
        }
        name.push(title.to_owned());
    }
    if name.is_empty() {
        name.push("Oneshot".to_owned());
    }
    Chapter {
        url: format!("/chapter/{}", chapter["id"].as_str().unwrap_or_default()),
        name: name.join(" "),
        date_upload: millis(a["publishAt"].as_str().unwrap_or_default()),
        chapter_number: a["chapter"].as_str().and_then(|n| n.parse().ok()).unwrap_or(-1.0),
        scanlator,
    }
}

/// One page of the chapter feed, newest first; chapters that cannot be read here are left out.
pub fn parse_feed(body: &str) -> Res<FeedPage> {
    let page = decode(body)?;
    let chapters = page["data"].as_array().into_iter().flatten().filter(|c| !is_invalid(&c["attributes"])).map(chapter_of).collect();
    let (limit, offset, total) = (page["limit"].as_i64().unwrap_or(0), page["offset"].as_i64().unwrap_or(0), page["total"].as_i64().unwrap_or(0));
    Ok(FeedPage { chapters, next_offset: (offset + limit < total && limit > 0).then_some(offset + limit) })
}

/// The page images of a chapter: the image server's address plus `/data/<hash>/<file>`, in order.
pub fn parse_pages(body: &str) -> Res<Vec<String>> {
    let page = decode(body)?;
    let base = page["baseUrl"].as_str().ok_or("MangaDex gave no image server")?.trim_end_matches('/');
    let hash = page["chapter"]["hash"].as_str().unwrap_or_default();
    Ok(page["chapter"]["data"].as_array().into_iter().flatten().filter_map(Value::as_str).map(|file| format!("{base}/data/{hash}/{file}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const POPULAR: &str = include_str!("../fixtures/popular.json");
    const LATEST: &str = include_str!("../fixtures/latest.json");
    const BY_IDS: &str = include_str!("../fixtures/byids.json");
    const DETAILS: &str = include_str!("../fixtures/details.json");
    const FEED: &str = include_str!("../fixtures/feed.json");
    const AT_HOME: &str = include_str!("../fixtures/athome.json");

    #[test]
    fn a_manga_list_has_english_titles_covers_and_a_next_page() {
        let page = parse_mangas(POPULAR).unwrap();
        assert_eq!(page.mangas.len(), 3);
        assert!(page.has_next_page);
        let first = &page.mangas[0];
        assert_eq!((first.url.as_str(), first.title.as_str()), ("/manga/32d76d19-8a05-4db0-9fc2-e0b0648fe9d0", "Solo Leveling"));
        assert!(first.thumbnail_url.as_deref().unwrap().starts_with("https://uploads.mangadex.org/covers/32d76d19-"));
    }

    #[test]
    fn latest_chapters_give_each_manga_once_in_order() {
        let (ids, more) = parse_latest_ids(LATEST).unwrap();
        assert_eq!(ids.len(), 4);
        assert!(more);
        let page = parse_in_order(BY_IDS, &ids[..1], more).unwrap();
        assert_eq!(page.mangas.len(), 1, "an id the answer lacks is left out");
        assert_eq!(page.mangas[0].url, format!("/manga/{}", ids[0]));
    }

    #[test]
    fn the_details_have_people_genres_status_and_summary() {
        let manga = parse_details(DETAILS).unwrap();
        assert_eq!(manga.title, "Solo Leveling");
        assert_eq!(manga.status, 2);
        assert!(manga.author.is_some() && manga.artist.is_some());
        assert!(manga.genre.unwrap().contains("Action"));
        assert!(!manga.description.unwrap().is_empty());
    }

    #[test]
    fn the_feed_names_chapters_with_volume_number_title_and_group() {
        let page = parse_feed(FEED).unwrap();
        assert_eq!(page.chapters.len(), 4);
        assert_eq!(page.next_offset, Some(4));
        let first = &page.chapters[0];
        assert!(first.name.contains("Ch.64"), "{}", first.name);
        assert_eq!(first.chapter_number, 64.0);
        assert!(first.url.starts_with("/chapter/") && first.date_upload > 0 && !first.scanlator.is_empty());
    }

    #[test]
    fn chapters_that_only_link_to_another_site_are_left_out() {
        let external = r#"{"result":"ok","limit":10,"offset":0,"total":2,"data":[
          {"id":"a","attributes":{"chapter":"1","externalUrl":"https://x.test","pages":0},"relationships":[]},
          {"id":"b","attributes":{"chapter":"2","externalUrl":null,"pages":9},"relationships":[]}]}"#;
        let page = parse_feed(external).unwrap();
        assert_eq!(page.chapters.len(), 1);
        assert_eq!(page.chapters[0].url, "/chapter/b");
        assert_eq!(page.next_offset, None);
    }

    #[test]
    fn the_pages_are_the_server_hash_and_file_names() {
        let pages = parse_pages(AT_HOME).unwrap();
        assert!(pages.len() > 1);
        assert!(pages[0].starts_with("https://cmdxd98sb0x3yprd.mangadex.network/data/c9a858704be01042d944f94f2457dd39/1-"));
    }

    #[test]
    fn an_api_error_says_what_it_was() {
        let error = r#"{"result":"error","errors":[{"detail":"The chapter is gone"}]}"#;
        assert_eq!(parse_pages(error).unwrap_err(), "MangaDex answered: The chapter is gone");
    }

    #[test]
    fn urls_carry_the_filters_and_check_ids() {
        assert!(search_url(2, "one piece").contains("title=one%20piece") && search_url(2, "x").contains("offset=20"));
        assert!(search_url(1, "").contains("order[followedCount]=desc"));
        assert!(details_url("/manga/not-an-id").is_err());
        assert!(feed_url("/manga/32d76d19-8a05-4db0-9fc2-e0b0648fe9d0", 500).unwrap().contains("offset=500"));
        assert!(at_home_url("/chapter/f7d2cb75-83b2-426b-bdbd-032870c30abb").is_ok());
    }

    #[test]
    fn dates_read_and_the_popular_window_is_thirty_days() {
        assert_eq!(millis("1970-01-02T00:00:01+00:00"), 86_401_000);
        assert_eq!(millis("1970-01-01T02:00:00+02:00"), 0, "the offset is applied");
        assert_eq!(thirty_days_before(1_791_936_000), "2026-09-14T00:00:00");
    }
}
