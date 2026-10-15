//! The AnimePahe site as pure functions: URLs and one parser per page or API answer. Ported from `AnimePahe.kt` and
//! `extractor/KwikExtractor.kt` (yuzono, Apache-2.0; the Kwik part is MIT, CloudStream). Fixed at the Kotlin defaults:
//! HLS links, "Sub" first, 1080p first, AV1 last, the site's own episode numbers off.

use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};

use crate::dto::{EpisodeDto, LatestAnimeDto, ResponseDto, SearchResultDto};

pub type Res<T> = Result<T, String>;

pub const BASE_URL: &str = "https://animepahe.pw";
pub const KWIK_ORIGIN: &str = "https://kwik.cx";

// ============================ Output (wire) ============================

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Anime {
    pub url: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub thumbnail_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    /// SAnime status: 0 unknown, 1 ongoing, 2 completed.
    #[serde(skip_serializing_if = "is_zero")]
    pub status: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
}

fn is_zero(n: &i32) -> bool {
    *n == 0
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimesPage {
    pub animes: Vec<Anime>,
    pub has_next_page: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Episode {
    pub url: String,
    pub name: String,
    pub episode_number: f32,
    /// Epoch milliseconds of `created_at`.
    pub date_upload: i64,
}

/// A fansub and audio of an episode. `internal_data` holds its streams, one `kwik link\tquality` per line; the
/// videos are read from it later, with the `videos` op.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hoster {
    pub hoster_url: String,
    pub hoster_name: String,
    #[serde(default)]
    pub internal_data: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Video {
    pub video_url: String,
    pub video_title: String,
    pub resolution: Option<i32>,
    pub headers: Vec<(String, String)>,
    pub preferred: bool,
}

// ================================ URLs =================================

/// An anime is kept as `/a/<id>`: the id never changes, while the site's session ids (in its page links) do every few
/// days. The session is cached by the plug-in and found again by a search when it no longer works.
pub fn anime_url(id: i64) -> String {
    format!("/a/{id}")
}

pub fn id_of(anime_url: &str) -> Option<i64> {
    anime_url.strip_prefix("/a/")?.split(['/', '?']).next()?.parse().ok()
}

/// A session from an old-style `/anime/<session>` url.
pub fn session_of(anime_url: &str) -> Option<&str> {
    anime_url.strip_prefix("/anime/").map(|rest| rest.split(['/', '?']).next().unwrap_or(rest)).filter(|s| !s.is_empty())
}

pub fn airing_url(page: i32) -> String {
    format!("{BASE_URL}/api?m=airing&page={page}")
}

/// A search. The site caches its API answers, so the Kotlin adds a changing number to the query, which the search
/// ignores; `stamp` is that number (Unix seconds).
pub fn search_url(query: &str, page: i32, stamp: u64) -> String {
    let q = format!("{query} {}", stamp + page as u64 * 3);
    format!("{BASE_URL}/api?m=search&q={}&page={page}", encode(&q))
}

pub fn details_url(session: &str) -> String {
    format!("{BASE_URL}/anime/{session}")
}

pub fn release_url(session: &str, page: i64) -> String {
    format!("{BASE_URL}/api?m=release&id={session}&sort=episode_asc&page={page}")
}

fn encode(text: &str) -> String {
    url::form_urlencoded::byte_serialize(text.as_bytes()).collect()
}

// =============================== Parsers ===============================

fn decode<T: serde::de::DeserializeOwned>(body: &str) -> Res<T> {
    serde_json::from_str(body).map_err(|e| format!("unexpected answer: {e}"))
}

/// The airing list (the site has no popular list; the Kotlin shows this one). Returns the page and each anime's
/// `(id, session)`, for the session cache.
pub fn parse_airing(body: &str) -> Res<(AnimesPage, Vec<(i64, String)>)> {
    let page: ResponseDto<LatestAnimeDto> = decode(body)?;
    let sessions = page.items.iter().map(|a| (a.id, a.session.clone())).collect();
    let animes = page
        .items
        .into_iter()
        .map(|a| Anime { url: anime_url(a.id), title: a.title, description: None, thumbnail_url: Some(a.snapshot), genre: None, status: 0, author: None })
        .collect();
    Ok((AnimesPage { animes, has_next_page: page.current_page < page.last_page }, sessions))
}

pub fn parse_search(body: &str) -> Res<(AnimesPage, Vec<(i64, String)>)> {
    let page: ResponseDto<SearchResultDto> = decode(body)?;
    let sessions = page.items.iter().map(|a| (a.id, a.session.clone())).collect();
    let animes = page
        .items
        .into_iter()
        .map(|a| Anime { url: anime_url(a.id), title: a.title, description: None, thumbnail_url: Some(a.poster), genre: None, status: 0, author: None })
        .collect();
    Ok((AnimesPage { animes, has_next_page: page.current_page < page.last_page }, sessions))
}

/// One result of a search: the anime's id, title, current session and poster.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub id: i64,
    pub title: String,
    pub session: String,
    pub poster: String,
}

/// The search results, for finding an anime's session again and its poster.
pub fn search_hits(body: &str) -> Res<Vec<Hit>> {
    let page: ResponseDto<SearchResultDto> = decode(body)?;
    Ok(page.items.into_iter().map(|a| Hit { id: a.id, title: a.title, session: a.session, poster: a.poster }).collect())
}

/// The title as a search query: letters, digits and spaces only (the site's search drops the rest).
pub fn query_of(title: &str) -> String {
    let q: String = title.chars().map(|c| if c.is_alphanumeric() || c.is_whitespace() { c } else { ' ' }).collect();
    q.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn sel(css: &str) -> Selector {
    Selector::parse(css).expect("a valid selector")
}

fn text_of(element: scraper::ElementRef) -> String {
    element.text().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The details page. The info lines are `<p><strong>Label: </strong>value</p>`.
pub fn parse_details(html: &str) -> Res<Anime> {
    let doc = Html::parse_document(html);
    let id = doc.select(&sel("meta[name=id]")).next().and_then(|m| m.value().attr("content")?.parse::<i64>().ok());
    let title = doc.select(&sel("div.title-wrapper > h1 > span")).next().map(text_of).ok_or("the details page has no title")?;
    let info: Vec<String> = doc.select(&sel("div.anime-info p")).map(text_of).collect();
    let line = |label: &str| info.iter().find(|l| l.starts_with(label)).map(|l| l[label.len()..].trim().to_owned()).filter(|v| !v.is_empty());
    let status = match line("Status:").as_deref() {
        Some("Currently Airing") => 1,
        Some("Finished Airing") => 2,
        _ => 0,
    };
    let mut genres: Vec<String> = doc.select(&sel("div.anime-genre ul li")).map(text_of).collect();
    genres.extend(["Demographic:", "Theme:"].iter().filter_map(|l| line(l)));
    let mut description = doc.select(&sel("div.anime-summary")).map(text_of).collect::<Vec<_>>().join(" ");
    for label in ["Synonyms:", "Japanese:", "Aired:", "Season:"] {
        if let Some(value) = line(label) {
            description.push_str(&format!("\n\n{label} {value}"));
        }
    }
    Ok(Anime {
        url: id.map(anime_url).unwrap_or_default(),
        title,
        description: Some(description.trim().to_owned()).filter(|d| !d.is_empty()),
        thumbnail_url: doc.select(&sel("div.anime-poster a")).next().and_then(|a| a.value().attr("href")).map(str::to_owned),
        genre: Some(genres.join(", ")).filter(|g| !g.is_empty()),
        status,
        author: line("Studios:"),
    })
}

/// One page of the episode list: its episodes (oldest first) and whether more pages follow.
pub fn parse_release(body: &str, session: &str) -> Res<(Vec<Episode>, bool)> {
    let page: ResponseDto<EpisodeDto> = decode(body)?;
    let more = page.current_page < page.last_page;
    let episodes = page
        .items
        .into_iter()
        .map(|e| Episode {
            url: format!("/play/{session}/{}", e.session),
            name: format!("Episode {}", number_text(e.episode_number)),
            episode_number: e.episode_number,
            date_upload: millis(&e.created_at),
        })
        .collect();
    Ok((episodes, more))
}

fn number_text(n: f32) -> String {
    if n.fract() == 0.0 { format!("{}", n as i64) } else { n.to_string() }
}

/// `yyyy-MM-dd HH:mm:ss` (UTC) as epoch milliseconds; 0 when it does not read.
pub fn millis(text: &str) -> i64 {
    let parse = || -> Option<i64> {
        let (date, time) = text.split_once(' ')?;
        let d: Vec<i64> = date.split('-').map(|p| p.parse().ok()).collect::<Option<_>>()?;
        let t: Vec<i64> = time.split(':').map(|p| p.parse().ok()).collect::<Option<_>>()?;
        let ([y, m, day], [hh, mm, ss]) = (d.as_slice().try_into().ok()?, t.as_slice().try_into().ok()?);
        // days from the civil date (Howard Hinnant's algorithm)
        let (y, m) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let doy = (153 * m + 2) / 5 + day - 1;
        let days = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
        Some(((days * 24 + hh) * 60 + mm) * 60_000 + ss * 1000)
    };
    parse().unwrap_or(0)
}

/// The language a stream's audio stands for, as the hoster names it.
fn language(audio: &str) -> &'static str {
    match audio {
        "eng" => "English",
        "chi" => "Chinese",
        "kor" => "Korean",
        _ => "Sub",
    }
}

/// The hosters of an episode's play page: one per fansub and language, in the page's order with "Sub" first. Each
/// holds its streams, best first.
pub fn parse_hosters(html: &str, play_url: &str) -> Vec<Hoster> {
    let doc = Html::parse_document(html);
    let mut hosters: Vec<(String, Vec<(i32, bool, String, String)>)> = Vec::new();
    for button in doc.select(&sel("div#resolutionMenu > button")) {
        let v = button.value();
        let Some(src) = v.attr("data-src").filter(|s| !s.is_empty()) else { continue };
        let fansub = v.attr("data-fansub").unwrap_or("AnimePahe");
        let resolution: i32 = v.attr("data-resolution").and_then(|r| r.parse().ok()).unwrap_or(0);
        let av1 = v.attr("data-av1") == Some("1");
        let name = format!("{fansub} ({})", language(v.attr("data-audio").unwrap_or_default()));
        let title = format!("{resolution}p{}", if av1 { " AV1" } else { "" });
        match hosters.iter_mut().find(|(n, _)| *n == name) {
            Some((_, streams)) => streams.push((resolution, av1, src.to_owned(), title)),
            None => hosters.push((name, vec![(resolution, av1, src.to_owned(), title)])),
        }
    }
    hosters.sort_by_key(|(name, _)| !name.ends_with("(Sub)"));
    hosters
        .into_iter()
        .map(|(name, mut streams)| {
            // 1080p first, then lower; at one height the non-AV1 stream first
            streams.sort_by_key(|(res, av1, ..)| (std::cmp::Reverse(*res), *av1));
            let internal_data = streams.iter().map(|(_, _, src, title)| format!("{src}\t{title}")).collect::<Vec<_>>().join("\n");
            Hoster { hoster_url: play_url.to_owned(), hoster_name: name, internal_data }
        })
        .collect()
}

/// The streams of a hoster: `(kwik link, quality title)`.
pub fn streams(internal_data: &str) -> Vec<(String, String)> {
    internal_data.lines().filter_map(|line| line.split_once('\t')).map(|(src, title)| (src.to_owned(), title.to_owned())).collect()
}

/// The HLS playlist named in a Kwik player page: its packed script, unpacked.
pub fn kwik_playlist(html: &str) -> Option<String> {
    let at = html.rfind("eval(function(")?;
    let script = &html[at..];
    let script = &script[..script.find("</script>").unwrap_or(script.len())];
    let unpacked = crate::unpacker::unpack_and_combine(script)?;
    crate::kwik::hls_source(&unpacked).map(str::to_owned)
}

/// The video of one stream: the playlist with the Kwik page as Referer, as the player's requests need.
pub fn video(playlist: String, title: &str, kwik_page: &str, preferred: bool) -> Video {
    let resolution = title.split('p').next().and_then(|n| n.parse().ok());
    Video {
        video_url: playlist,
        video_title: title.to_owned(),
        resolution,
        headers: vec![("Referer".into(), kwik_page.to_owned()), ("Origin".into(), KWIK_ORIGIN.into())],
        preferred,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AIRING: &str = include_str!("../fixtures/airing.json");
    const SEARCH: &str = include_str!("../fixtures/search.json");
    const ANIME: &str = include_str!("../fixtures/anime.html");
    const RELEASE: &str = include_str!("../fixtures/release.json");
    const PLAY: &str = include_str!("../fixtures/play.html");
    const KWIK: &str = include_str!("../fixtures/kwik.html");

    #[test]
    fn the_airing_list_keeps_ids_and_their_sessions() {
        let (page, sessions) = parse_airing(AIRING).unwrap();
        assert_eq!(page.animes.len(), 12);
        assert!(page.has_next_page);
        let first = &page.animes[0];
        assert_eq!((first.url.as_str(), first.title.as_str()), ("/a/6862", "A Returner's Magic Should Be Special Season 2"));
        assert_eq!(sessions[0], (6862, "bf0d2a6b-73bc-b740-3c38-d5214a945307".to_owned()));
    }

    #[test]
    fn a_search_lists_posters_and_hits() {
        let (page, _) = parse_search(SEARCH).unwrap();
        assert_eq!(page.animes[0].title, "Frieren: Beyond Journey's End");
        assert!(page.animes[0].thumbnail_url.as_deref().unwrap().contains("/posters/"));
        assert!(!page.has_next_page);
        let hits = search_hits(SEARCH).unwrap();
        assert_eq!(hits[0].id, 5319);
        assert!(hits[0].poster.contains("/posters/"));
        assert_eq!(query_of("Frieren: Beyond Journey's End!"), "Frieren Beyond Journey s End");
    }

    #[test]
    fn the_details_page_reads_title_status_poster_and_summary() {
        let anime = parse_details(ANIME).unwrap();
        assert_eq!(anime.url, "/a/6862");
        assert_eq!(anime.title, "A Returner's Magic Should Be Special Season 2");
        assert_eq!(anime.status, 1);
        assert!(anime.thumbnail_url.unwrap().ends_with(".webp"));
        assert!(anime.description.unwrap().contains("Japanese:"));
    }

    #[test]
    fn the_episode_list_links_each_play_page() {
        let (episodes, more) = parse_release(RELEASE, "S").unwrap();
        assert!(!more);
        assert_eq!(episodes[0].name, "Episode 13");
        assert!(episodes[0].url.starts_with("/play/S/c3dc453a"));
        assert_eq!(episodes[0].date_upload, millis("2026-10-07 17:26:43"));
        assert_eq!(millis("1970-01-02 00:00:01"), 86_401_000);
    }

    #[test]
    fn the_play_page_groups_streams_per_fansub_best_first() {
        let hosters = parse_hosters(PLAY, "/play/a/b");
        assert!(hosters[0].hoster_name.ends_with("(Sub)"), "{:?}", hosters[0].hoster_name);
        let streams = streams(&hosters[0].internal_data);
        assert_eq!(streams[0].1, "1080p");
        assert!(streams[0].0.starts_with("https://kwik.cx/e/"));
    }

    #[test]
    fn the_kwik_page_names_an_hls_playlist() {
        let playlist = kwik_playlist(KWIK).expect("a playlist");
        assert!(playlist.starts_with("https://") && playlist.contains(".m3u8"), "{playlist}");
    }

    #[test]
    fn ids_and_sessions_come_out_of_anime_urls() {
        assert_eq!(id_of("/a/6862"), Some(6862));
        assert_eq!(id_of("/anime/x"), None);
        assert_eq!(session_of("/anime/abc-1"), Some("abc-1"));
    }
}
