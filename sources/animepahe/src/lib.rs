//! AnimePahe (<https://animepahe.pw>) as a Kurobayomi source plug-in, ABI 1.5 (`wit/kuro-source.wit`), anime ops only.
//! Ported from the yuzono `en/animepahe` extension (Apache-2.0; the Kwik decode is MIT, CloudStream); see README.md. The
//! site answers plain clients with Cloudflare's browser check: the app then sends its requests from a hidden page, so
//! nothing here deals with it. Every network call goes through the host's `http` import, limited to `manifest.toml`.

mod api;
mod dto;
mod kwik;
mod unpacker;

wit_bindgen::generate!({
    path: "wit",
    world: "source",
});

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;

use api::{AnimesPage, Hoster, Res};
use kuro::source::host;

/// The source id: the MD5 of `animepahe/en/1`, as the Kotlin extension uses. Keep it: backups name it.
const ID: i64 = 9163675745699695675;
const NAME: &str = "AnimePahe";
const LANG: &str = "en";
/// The site answers too many quick requests with 429; the Kotlin waits 3 s between pages.
const REQUEST_GAP_MS: u64 = 1500;

// ================================ HTTP =================================

#[derive(Serialize)]
struct HttpRequest<'a> {
    method: &'a str,
    url: &'a str,
    headers: &'a [(&'a str, &'a str)],
    body: Option<&'a str>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HttpResponse {
    status: u16,
    #[serde(default)]
    url: String,
    body: String,
    #[serde(default)]
    body_encoding: Option<String>,
}

/// One GET through the host: the body and the address it ended at. A non-2xx status is an error.
fn fetch(url: &str, headers: &[(&str, &str)]) -> Res<(String, String)> {
    let request = serde_json::to_string(&HttpRequest { method: "GET", url, headers, body: None }).map_err(|e| e.to_string())?;
    let response: HttpResponse = serde_json::from_str(&host::http(&request)?).map_err(|e| format!("invalid response: {e}"))?;
    if response.status == 429 {
        return Err("AnimePahe is limiting requests; wait a few seconds and try again".into());
    }
    if !(200..300).contains(&response.status) {
        return Err(format!("HTTP error {}", response.status));
    }
    if response.body_encoding.is_some() {
        return Err(format!("{url} sent a binary body"));
    }
    Ok((response.body, response.url))
}

fn get(url: &str) -> Res<String> {
    fetch(url, &[("Referer", "https://animepahe.pw/")]).map(|(body, _)| body)
}

// =========================== Session cache =============================

fn session_key(id: i64) -> String {
    format!("session_{id}")
}

fn remember(sessions: &[(i64, String)]) {
    for (id, session) in sessions {
        if cached_session(*id).as_deref() != Some(session) {
            let _ = host::pref_set(&session_key(*id), &json!(session).to_string());
        }
    }
}

fn cached_session(id: i64) -> Option<String> {
    serde_json::from_str(&host::pref_get(&session_key(id))).ok()
}

fn unix_seconds() -> u64 {
    host::unix_seconds()
}

/// Finds an anime's current session by searching its title and matching its id (Kotlin `fetchSessionAndId`).
fn find_session(id: i64, title: &str) -> Res<String> {
    let query: String = title.chars().map(|c| if c.is_alphanumeric() || c.is_whitespace() { c } else { ' ' }).collect();
    let query = query.split_whitespace().collect::<Vec<_>>().join(" ");
    for page in 1..=3 {
        let hits = api::search_hits(&get(&api::search_url(&query, page, unix_seconds()))?)?;
        if let Some((_, _, session)) = hits.iter().find(|(hit, ..)| *hit == id) {
            remember(&[(id, session.clone())]);
            return Ok(session.clone());
        }
        if hits.is_empty() {
            break;
        }
    }
    Err(format!("AnimePahe no longer lists \"{title}\"; search for it again"))
}

/// The session of an anime: the cached one, or a fresh one when `stale` (the cached one failed) or none is cached.
fn session(anime: &AnimeRef, stale: bool) -> Res<String> {
    if let Some(session) = api::session_of(&anime.url) {
        return Ok(session.to_owned());
    }
    let id = api::id_of(&anime.url).ok_or_else(|| format!("not an AnimePahe entry: {}", anime.url))?;
    match cached_session(id).filter(|_| !stale) {
        Some(session) => Ok(session),
        None => find_session(id, &anime.title),
    }
}

/// Runs `op` with the anime's session; when the site no longer knows it, finds it again and retries once.
fn with_session<T>(anime: &AnimeRef, op: impl Fn(&str) -> Res<T>) -> Res<T> {
    match op(&session(anime, false)?) {
        Err(error) if error.starts_with("HTTP error 404") && api::id_of(&anime.url).is_some() => op(&session(anime, true)?),
        result => result,
    }
}

// ================================ Ops ==================================

#[derive(Deserialize)]
struct Envelope<T> {
    args: T,
}

fn args<T: DeserializeOwned>(input: &str) -> Res<T> {
    serde_json::from_str::<Envelope<T>>(input).map(|e| e.args).map_err(|e| format!("invalid args: {e}"))
}

#[derive(Deserialize)]
struct PageArgs {
    page: i32,
}

#[derive(Deserialize)]
struct SearchArgs {
    page: i32,
    query: String,
}

#[derive(Deserialize)]
struct AnimeRef {
    url: String,
    #[serde(default)]
    title: String,
}

#[derive(Deserialize)]
struct AnimeArgs {
    anime: AnimeRef,
}

#[derive(Deserialize)]
struct EpisodeRef {
    url: String,
}

#[derive(Deserialize)]
struct EpisodeArgs {
    episode: EpisodeRef,
}

#[derive(Deserialize)]
struct VideosArgs {
    hoster: Option<Hoster>,
    episode: Option<EpisodeRef>,
}

fn descriptor() -> serde_json::Value {
    json!({ "id": ID, "name": NAME, "lang": LANG, "supportsLatest": false, "requestGapMs": REQUEST_GAP_MS })
}

/// The airing list: the site has no popular list, and the Kotlin shows this one.
fn popular(input: &str) -> Res<AnimesPage> {
    let page = args::<PageArgs>(input)?.page;
    let (list, sessions) = api::parse_airing(&get(&api::airing_url(page))?)?;
    remember(&sessions);
    Ok(list)
}

fn search(input: &str) -> Res<AnimesPage> {
    let a: SearchArgs = args(input)?;
    if a.query.trim().is_empty() {
        return popular(&json!({ "args": { "page": a.page } }).to_string());
    }
    let (list, sessions) = api::parse_search(&get(&api::search_url(a.query.trim(), a.page, unix_seconds()))?)?;
    remember(&sessions);
    Ok(list)
}

fn details(input: &str) -> Res<api::Anime> {
    let anime = args::<AnimeArgs>(input)?.anime;
    let mut found = with_session(&anime, |session| api::parse_details(&get(&api::details_url(session))?))?;
    if found.url.is_empty() {
        found.url = anime.url;
    }
    Ok(found)
}

/// Every page of the episode list, newest first.
fn episodes(input: &str) -> Res<Vec<api::Episode>> {
    let anime = args::<AnimeArgs>(input)?.anime;
    let mut list = with_session(&anime, |session| {
        let mut all = Vec::new();
        for page in 1.. {
            let (episodes, more) = api::parse_release(&get(&api::release_url(session, page))?, session)?;
            all.extend(episodes);
            if !more {
                break;
            }
        }
        Ok(all)
    })?;
    list.reverse();
    Ok(list)
}

fn hosters(input: &str) -> Res<Vec<Hoster>> {
    let play = args::<EpisodeArgs>(input)?.episode.url;
    let url = format!("{}{}", api::BASE_URL, play.split('?').next().unwrap_or(&play));
    Ok(api::parse_hosters(&get(&url)?, &play))
}

/// The videos of a hoster: each stream's Kwik page, unpacked to its HLS playlist. A stream that fails is skipped.
fn hoster_videos(hoster: &Hoster, first: bool) -> Vec<api::Video> {
    let mut videos = Vec::new();
    for (kwik, title) in api::streams(&hoster.internal_data) {
        let found = fetch(&kwik, &[("Referer", "https://animepahe.pw/")])
            .and_then(|(html, landed)| api::kwik_playlist(&html).map(|p| (p, landed)).ok_or_else(|| "no playlist in the Kwik page".to_owned()));
        match found {
            Ok((playlist, landed)) => {
                let preferred = first && videos.is_empty();
                videos.push(api::video(playlist, &title, if landed.is_empty() { &kwik } else { &landed }, preferred));
            }
            Err(error) => host::log(2, &format!("animepahe: {title} of {} failed: {error}", hoster.hoster_name)),
        }
    }
    videos
}

fn videos(input: &str) -> Res<Vec<api::Video>> {
    let a: VideosArgs = args(input)?;
    if let Some(hoster) = a.hoster {
        return Ok(hoster_videos(&hoster, true));
    }
    let episode = a.episode.ok_or("videos needs a hoster or an episode")?;
    let hosters = hosters(&json!({ "args": { "episode": { "url": episode.url } } }).to_string())?;
    Ok(hosters.iter().enumerate().flat_map(|(i, h)| hoster_videos(h, i == 0)).collect())
}

fn json_out<T: Serialize>(result: Res<T>) -> Res<String> {
    result.and_then(|value| serde_json::to_string(&value).map_err(|e| e.to_string()))
}

struct Plugin;

impl Guest for Plugin {
    fn invoke(op: String, input_json: String) -> Result<String, String> {
        match op.as_str() {
            "info" => Ok(descriptor().to_string()),
            "create-sources" => Ok(json!([descriptor()]).to_string()),
            "popular" => json_out(popular(&input_json)),
            "search" => json_out(search(&input_json)),
            "details" => json_out(details(&input_json)),
            "episodes" => json_out(episodes(&input_json)),
            "hosters" => json_out(hosters(&input_json)),
            "videos" => json_out(videos(&input_json)),
            _ => Err(format!("unknown op: {op}")),
        }
    }
}

export!(Plugin);
