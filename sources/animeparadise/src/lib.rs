//! AnimeParadise (<https://www.animeparadise.moe>), an anime source plug-in for Kurobayomi, ABI 1.5
//! (`wit/kuro-source.wit`). Ported from the Kohi-den `en/animeparadise` extension (Apache-2.0) and brought up to the site's
//! current API (see `api.rs`). This file is the ABI glue: the ops and the one network path, the `http` import.

mod api;

use serde::Deserialize;
use serde_json::{Value, json};

wit_bindgen::generate!({
    path: "wit",
    world: "source",
});

use api::Res;
use kuro::source::host;

/// The source id: the MD5 of `animeparadise/en/1`, as the Kotlin extension uses. Keep it: backups name it.
const ID: i64 = 3319373396687010408;
const NAME: &str = "AnimeParadise";
const LANG: &str = "en";
const REQUEST_GAP_MS: u64 = 300;

/// The `args` of an op. Each op reads only the fields it needs.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Args {
    page: i32,
    query: String,
    anime: Option<Ref>,
    episode: Option<Ref>,
    hoster: Option<HosterRef>,
}

#[derive(Deserialize)]
struct Ref {
    url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HosterRef {
    hoster_url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Envelope {
    #[serde(default)]
    args: Args,
    /// The app's "Show adult (18+) titles" switch. An app that does not send it hides them.
    #[serde(default)]
    show_adult: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HttpResponse {
    status: u16,
    body: String,
    #[serde(default)]
    body_encoding: Option<String>,
}

fn descriptor() -> Value {
    json!({ "id": ID, "name": NAME, "lang": LANG, "supportsLatest": true, "requestGapMs": REQUEST_GAP_MS })
}

/// One GET to the API through the host, which refuses any host outside `hosts` in `manifest.toml`. A non-2xx answer is
/// an error: the API's own message when it gives one.
fn get(url: &str) -> Res<String> {
    let site = format!("{}/", api::SITE);
    let request = json!({
        "method": "GET",
        "url": url,
        "headers": [["Accept", "application/json, text/plain, */*"], ["Origin", api::SITE], ["Referer", site]],
    });
    let response: HttpResponse = serde_json::from_str(&host::http(&request.to_string())?).map_err(|e| format!("invalid http reply: {e}"))?;
    if response.body_encoding.is_some() {
        return Err(format!("{url} sent a binary body"));
    }
    match response.status {
        200..=299 => Ok(response.body),
        429 => Err("AnimeParadise is limiting requests; wait a few seconds and try again".into()),
        status => Err(api::error_message(&response.body).unwrap_or_else(|| format!("HTTP {status} from AnimeParadise"))),
    }
}

fn to_value<T: serde::Serialize>(value: T) -> Res<Value> {
    serde_json::to_value(value).map_err(|e| e.to_string())
}

/// The episode's one hoster, its video already read.
fn hoster_of(episode_url: &str) -> Res<api::Hoster> {
    let video = api::parse_video(&get(&api::episode_api_url(episode_url)?)?)?;
    Ok(api::Hoster { hoster_url: episode_url.to_owned(), hoster_name: NAME.to_owned(), video_list: vec![video] })
}

fn run(op: &str, args: Args, show_adult: bool) -> Res<Value> {
    match op {
        // the site's own lists: by popularity, and by when an anime was posted
        "popular" => to_value(api::parse_list(&get(&api::search_url(args.page, "", "POPULARITY"))?, show_adult)?),
        "latest" => to_value(api::parse_list(&get(&api::search_url(args.page, "", "POST_DATE"))?, show_adult)?),
        "search" => to_value(api::parse_list(&get(&api::search_url(args.page, &args.query, ""))?, show_adult)?),
        "details" => {
            let anime = args.anime.ok_or("details needs args.anime")?;
            to_value(api::parse_details(&get(&api::details_url(&api::link_of(&anime.url)?.slug))?)?)
        }
        "episodes" => {
            let anime = args.anime.ok_or("episodes needs args.anime")?;
            to_value(api::parse_episodes(&get(&api::episodes_url(&api::link_of(&anime.url)?.id))?)?)
        }
        "hosters" => to_value(vec![hoster_of(&args.episode.ok_or("hosters needs args.episode")?.url)?]),
        "videos" => {
            let url = match (args.hoster, args.episode) {
                (Some(hoster), _) => hoster.hoster_url,
                (None, Some(episode)) => episode.url,
                (None, None) => return Err("videos needs a hoster or an episode".into()),
            };
            to_value(hoster_of(&url)?.video_list)
        }
        _ => Err(format!("unknown op: {op}")),
    }
}

struct Plugin;

impl Guest for Plugin {
    fn invoke(op: String, input_json: String) -> Result<String, String> {
        let out = match op.as_str() {
            "info" => descriptor(),
            "create-sources" => Value::Array(vec![descriptor()]),
            // ABI 1.5: the headers of the stream and subtitle requests
            "request-headers" => to_value(api::media_headers())?,
            "popular" | "latest" | "search" | "details" | "episodes" | "hosters" | "videos" => {
                let envelope: Envelope = serde_json::from_str(&input_json).map_err(|e| format!("invalid input: {e}"))?;
                run(&op, envelope.args, envelope.show_adult)?
            }
            _ => return Err(format!("unknown op: {op}")),
        };
        Ok(out.to_string())
    }
}

export!(Plugin);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_carries_the_kotlin_id() {
        let info: Value = serde_json::from_str(&<Plugin as Guest>::invoke("info".into(), "{}".into()).unwrap()).unwrap();
        assert_eq!(info["id"].as_i64(), Some(ID));
        assert_eq!(info["supportsLatest"], true);
    }

    #[test]
    fn the_media_headers_name_the_site() {
        let headers: Value = serde_json::from_str(&<Plugin as Guest>::invoke("request-headers".into(), "{}".into()).unwrap()).unwrap();
        assert_eq!(headers, json!([["Referer", "https://www.animeparadise.moe/"], ["Origin", "https://www.animeparadise.moe"]]));
    }

    #[test]
    fn an_op_the_plug_in_does_not_answer_is_unknown() {
        assert_eq!(<Plugin as Guest>::invoke("pages".into(), "{}".into()), Err("unknown op: pages".to_owned()));
    }
}
