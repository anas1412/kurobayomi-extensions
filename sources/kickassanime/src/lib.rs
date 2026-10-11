//! KickAssAnime (<https://kaa.lt>) as a Kurobayomi source plug-in, ABI 1.5 (`wit/kuro-source.wit`), anime ops only.
//! The logic is ported from the yuzono `en/kickassanime` extension (Apache-2.0); see README.md. Every network call goes
//! through the host's `http` import, which refuses hosts outside `manifest.toml`.

mod api;
mod extract;

wit_bindgen::generate!({
    path: "wit",
    world: "source",
});

use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use api::{AnimeRef, AnimesPage, EpisodeRef, Hoster, Res};
use extract::{Track, Video};
use kuro::source::host;

/// The source id: the MD5 of `kickassanime/en/1`, as the Kotlin extension and Kurobayomi's built-in id use. Keep it.
const ID: i64 = 2081771513481539599;
const NAME: &str = "KickAssAnime";
const LANG: &str = "en";
/// The Kotlin settings at their defaults. The app has no settings store for plug-ins yet.
const AUDIO_FIRST: &str = "ja-JP";
const AUDIO_SECOND: &str = "en-US";
const QUALITY: &str = "1080p";
/// The preferred server (the first of the Kotlin `SERVERS`).
const SERVER: &str = "VidStreaming";

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
    body: String,
    #[serde(default)]
    body_encoding: Option<String>,
}

/// Sends one request through the host. A non-2xx status is an error, as in the Kotlin client.
fn send(method: &str, url: &str, headers: &[(&str, &str)], body: Option<&str>) -> Res<String> {
    let request = serde_json::to_string(&HttpRequest { method, url, headers, body }).map_err(|e| e.to_string())?;
    let text = host::http(&request)?;
    let response: HttpResponse = serde_json::from_str(&text).map_err(|e| format!("invalid response: {e}"))?;
    if response.body_encoding.is_some() {
        return Err(format!("{url} sent a binary body"));
    }
    if !(200..300).contains(&response.status) {
        return Err(format!("HTTP error {}", response.status));
    }
    Ok(response.body)
}

fn get(url: &str, headers: &[(&str, &str)]) -> Res<String> {
    send("GET", url, headers, None)
}

/// The request headers of a video: the same ones the player sends (see `extract::video_headers`).
fn borrowed(headers: &[(String, String)]) -> Vec<(&str, &str)> {
    headers.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect()
}

fn warn(message: String) {
    host::log(2, &message);
}

// ================================ Ops ==================================

/// The `args` of an op inside the `{"sourceId", "args"}` envelope.
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
    /// The app's Filter list (ABI 1.5): the serde form of each filter. Empty when the app holds none.
    #[serde(default)]
    filters: Vec<Value>,
}

#[derive(Deserialize)]
struct AnimeArgs {
    anime: AnimeRef,
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

#[derive(Deserialize)]
struct VideoArgs {
    video: VideoIn,
}

/// The part of a video the `subtitles` op reads.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VideoIn {
    #[serde(default)]
    subtitle_tracks: Vec<Track>,
}

fn descriptor() -> Value {
    json!({ "id": ID, "name": NAME, "lang": LANG, "supportsLatest": true })
}

fn popular(input: &str) -> Res<AnimesPage> {
    let a: PageArgs = args(input)?;
    api::parse_popular(&get(&api::popular_url(a.page), &[])?, a.page)
}

fn latest(input: &str) -> Res<AnimesPage> {
    let a: PageArgs = args(input)?;
    api::parse_latest(&get(&api::latest_url(a.page), &[])?)
}

/// A search (Kotlin `getSearchAnime`): a slug or a link names one show; a sub-page filter replaces the search; no text
/// browses with the filters; text posts them with the query. The Referer is the search page of the query.
fn search(input: &str) -> Res<AnimesPage> {
    let a: SearchArgs = args(input)?;
    if let Some(slug) = api::slug_of_query(&a.query)? {
        let anime = details(&format!("/{slug}"))?;
        return Ok(AnimesPage { animes: vec![anime], has_next_page: false });
    }
    let referer = api::search_referer(&a.query);
    let headers = [
        ("Accept", "application/json, text/plain, */*"),
        ("Referer", referer.as_str()),
        ("Content-Type", "application/json"),
    ];
    if let Some(subpage) = api::subpage(&a.filters) {
        let body = get(&api::subpage_url(subpage, a.page), &headers)?;
        return api::parse_subpage(subpage, &body, a.page);
    }
    let filters = api::encoded_filters(&a.filters);
    if a.query.trim().is_empty() {
        let body = get(&api::browse_url(a.page, filters.as_deref()), &headers)?;
        api::parse_search(&body, a.page)
    } else {
        let body = send("POST", api::FSEARCH_URL, &headers, Some(&api::search_body(a.page, &a.query, filters.as_deref())))?;
        api::parse_search(&body, a.page)
    }
}

fn languages(anime_url: &str) -> Res<Vec<String>> {
    api::parse_languages(&get(&api::language_url(anime_url), &[])?)
}

/// The details of one show, read for a search by slug or a link.
fn details(anime_url: &str) -> Res<api::Anime> {
    let body = get(&api::details_url(anime_url), &[])?;
    api::parse_details(&body, &languages(anime_url)?)
}

/// The episodes in the first language that has any: the preferred one, the second, then the site's order. Newest
/// first. A page after the first that fails is skipped.
fn episodes(input: &str) -> Res<Vec<api::Episode>> {
    let anime = args::<AnimeArgs>(input)?.anime.url;
    for lang in api::language_order(languages(&anime)?, AUDIO_FIRST, AUDIO_SECOND) {
        let first = match get(&api::episodes_url(&anime, 1, &lang), &[]).and_then(|b| api::parse_episode_page(&b)) {
            Ok(page) if !page.is_empty() => page,
            _ => continue,
        };
        let mut list = first.episodes(&anime, &lang);
        for number in 2..=first.page_count() {
            match get(&api::episodes_url(&anime, number, &lang), &[]).and_then(|b| api::parse_episode_page(&b)) {
                Ok(page) => list.extend(page.episodes(&anime, &lang)),
                Err(err) => warn(format!("kickassanime: episode page {number} failed: {err}")),
            }
        }
        list.reverse();
        return Ok(list);
    }
    Ok(Vec::new())
}

fn servers(episode_url: &str) -> Res<Vec<api::Server>> {
    api::parse_servers(&get(&api::servers_url(episode_url), &[])?)
}

/// One hoster per server, in the site's order. Its videos are read later, with the `videos` op.
fn hosters(input: &str) -> Res<Vec<Hoster>> {
    let episode = args::<EpisodeArgs>(input)?.episode.url;
    Ok(servers(&episode)?.into_iter().map(|s| Hoster { hoster_url: s.src, hoster_name: s.name }).collect())
}

/// The videos of one server: its player page, then the manifest it names, split per quality with the subtitles.
fn server_videos(name: &str, src: &str) -> Res<Vec<Video>> {
    let player = extract::player_url(src)?;
    let html = get(player.as_str(), &[])?;
    let Some(data) = extract::parse_new_player(&html, &player) else {
        warn(format!("kickassanime: {name} is not the current player; no videos"));
        return Ok(Vec::new());
    };
    let headers = extract::video_headers(&player);
    let dash = !data.manifest.contains(".m3u8");
    let text = match get(&data.manifest, &borrowed(&headers)) {
        Ok(text) => text,
        Err(err) => {
            warn(format!("kickassanime: manifest of {name} could not be fetched: {err}"));
            return Ok(Vec::new());
        }
    };
    Ok(if dash {
        extract::dash_videos(name, &data.manifest, &text, &data.subtitles, &headers)
    } else {
        extract::hls_videos(name, &data.manifest, &text, &data.subtitles, &headers)
    })
}

/// The videos of one hoster, sorted; or, for an episode, every server's videos sorted the same way (a failing server is
/// skipped). The Kotlin sorts the videos of an episode in `sortVideos` too.
fn videos(input: &str) -> Res<Vec<Video>> {
    let a: VideosArgs = args(input)?;
    if let Some(hoster) = a.hoster {
        let found = server_videos(&hoster.hoster_name, &hoster.hoster_url)?;
        return Ok(extract::sort_videos(found, SERVER, QUALITY));
    }
    let Some(episode) = a.episode else {
        return Err("videos needs a hoster or an episode".into());
    };
    let mut all = Vec::new();
    for server in servers(&episode.url)? {
        match server_videos(&server.name, &server.src) {
            Ok(found) => all.extend(found),
            Err(err) => warn(format!("kickassanime: server {} failed: {err}", server.name)),
        }
    }
    Ok(extract::sort_videos(all, SERVER, QUALITY))
}

fn json_out<T: serde::Serialize>(result: Res<T>) -> Res<String> {
    result.and_then(|value| serde_json::to_string(&value).map_err(|e| e.to_string()))
}

struct Plugin;

impl Guest for Plugin {
    fn invoke(op: String, input_json: String) -> Result<String, String> {
        match op.as_str() {
            "info" => Ok(descriptor().to_string()),
            "create-sources" => Ok(json!([descriptor()]).to_string()),
            // ABI 1.5: the headers of this source's video requests. The Origin is left out: it is the player's host, which
            // each video carries in its own `headers` (`extract::video_headers`).
            "request-headers" => Ok(json!(extract::common_headers()).to_string()),
            "popular" => json_out(popular(&input_json)),
            "latest" => json_out(latest(&input_json)),
            "search" => json_out(search(&input_json)),
            "episodes" => json_out(episodes(&input_json)),
            "hosters" => json_out(hosters(&input_json)),
            "videos" => json_out(videos(&input_json)),
            "subtitles" => json_out(args::<VideoArgs>(&input_json).map(|a| a.video.subtitle_tracks)),
            _ => Err(format!("unknown op: {op}")),
        }
    }
}

export!(Plugin);

#[cfg(test)]
mod tests {
    use super::*;

    /// ABI 1.5 `request-headers`: the player's headers, with neither Referer nor Origin (each video has its Origin).
    #[test]
    fn the_request_headers_are_the_player_headers_without_referer_or_origin() {
        let headers = <Plugin as Guest>::invoke("request-headers".into(), "{}".into()).unwrap();
        let pairs: Vec<(String, String)> = serde_json::from_str(&headers).unwrap();
        assert!(pairs.iter().all(|(k, _)| !k.eq_ignore_ascii_case("origin") && !k.eq_ignore_ascii_case("referer")));
        assert!(pairs.iter().any(|(k, v)| k == "User-Agent" && v == extract::VIDEO_USER_AGENT));
    }

    #[test]
    fn an_op_the_plug_in_does_not_answer_is_unknown() {
        assert_eq!(
            <Plugin as Guest>::invoke("chapters".into(), "{}".into()),
            Err("unknown op: chapters".to_owned())
        );
    }
}
