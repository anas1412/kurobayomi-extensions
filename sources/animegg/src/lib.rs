//! AnimeGG (<https://www.animegg.org>), an anime source plug-in for Kurobayomi, ABI 1.5 (`wit/kuro-source.wit`). Ported
//! from the Kohi-den `en/animegg` extension (Apache-2.0). This file is the ABI glue: the ops and the one network path, the
//! `http` import. The selectors and parsers are in `api.rs`.

mod api;

use serde::Deserialize;
use serde_json::{Value, json};

wit_bindgen::generate!({
    path: "wit",
    world: "source",
});

use api::{Hoster, Res};
use kuro::source::host;

/// The source id: the MD5 of `animegg/en/1`, as the Kotlin extension uses. Keep it: backups name it.
const ID: i64 = 2247987965040486818;
const NAME: &str = "AnimeGG";
const LANG: &str = "en";
const REQUEST_GAP_MS: u64 = 300;

/// The `args` of an op. Each op reads only the fields it needs.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Args {
    query: String,
    anime: Option<Ref>,
    episode: Option<Ref>,
    hoster: Option<Hoster>,
}

#[derive(Deserialize)]
struct Ref {
    url: String,
}

#[derive(Deserialize)]
struct Envelope {
    #[serde(default)]
    args: Args,
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

/// One GET of a site page through the host, which refuses any host outside `hosts` in `manifest.toml`.
fn get(url: &str, referer: &str) -> Res<String> {
    let request = json!({ "method": "GET", "url": url, "headers": [["Referer", referer]] });
    let response: HttpResponse = serde_json::from_str(&host::http(&request.to_string())?).map_err(|e| format!("invalid http reply: {e}"))?;
    if response.body_encoding.is_some() {
        return Err(format!("{url} sent a binary body"));
    }
    match response.status {
        200..=299 => Ok(response.body),
        429 => Err("AnimeGG is limiting requests; wait a few seconds and try again".into()),
        status => Err(format!("HTTP {status} from AnimeGG")),
    }
}

fn page(url: &str) -> Res<String> {
    get(url, &format!("{}/", api::BASE))
}

fn to_value<T: serde::Serialize>(value: T) -> Res<Value> {
    serde_json::to_value(value).map_err(|e| e.to_string())
}

/// The videos of one player. A player whose page fails gives none, so the others still play (the Kotlin's
/// `parallelCatchingFlatMap`).
fn hoster_videos(hoster: &Hoster, episode_url: &str, preferred: bool) -> Vec<api::Video> {
    match get(&hoster.hoster_url, episode_url) {
        Ok(html) => api::parse_embed(&html, &hoster.hoster_url, &hoster.hoster_name, preferred),
        Err(error) => {
            host::log(2, &format!("animegg: {} failed: {error}", hoster.hoster_name));
            Vec::new()
        }
    }
}

fn run(op: &str, args: Args) -> Res<Value> {
    match op {
        "popular" => to_value(api::parse_list(&page(&api::popular_url())?)),
        "latest" => to_value(api::parse_list(&page(&api::latest_url())?)),
        // the Kotlin shows the popular list for an empty search
        "search" if args.query.trim().is_empty() => to_value(api::parse_list(&page(&api::popular_url())?)),
        "search" => to_value(api::parse_search(&page(&api::search_url(&args.query))?)),
        "details" => {
            let anime = args.anime.ok_or("details needs args.anime")?;
            to_value(api::parse_details(&page(&api::absolute(&anime.url))?, &anime.url))
        }
        "episodes" => to_value(api::parse_episodes(&page(&api::absolute(&args.anime.ok_or("episodes needs args.anime")?.url))?)),
        "hosters" => to_value(api::parse_hosters(&page(&api::absolute(&args.episode.ok_or("hosters needs args.episode")?.url))?)),
        "videos" => match (args.hoster, args.episode) {
            (Some(hoster), _) => to_value(hoster_videos(&hoster, &format!("{}/", api::BASE), true)),
            (None, Some(episode)) => {
                let url = api::absolute(&episode.url);
                let hosters = api::parse_hosters(&page(&url)?);
                to_value(hosters.iter().enumerate().flat_map(|(i, h)| hoster_videos(h, &url, i == 0)).collect::<Vec<_>>())
            }
            (None, None) => Err("videos needs a hoster or an episode".into()),
        },
        _ => Err(format!("unknown op: {op}")),
    }
}

struct Plugin;

impl Guest for Plugin {
    fn invoke(op: String, input_json: String) -> Result<String, String> {
        let out = match op.as_str() {
            "info" => descriptor(),
            "create-sources" => Value::Array(vec![descriptor()]),
            // ABI 1.5: the video host answers 500 without the site as referer
            "request-headers" => to_value(api::video_headers())?,
            "popular" | "latest" | "search" | "details" | "episodes" | "hosters" | "videos" => {
                let envelope: Envelope = serde_json::from_str(&input_json).map_err(|e| format!("invalid input: {e}"))?;
                run(&op, envelope.args)?
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
    }

    #[test]
    fn the_media_headers_name_the_site() {
        let headers: Value = serde_json::from_str(&<Plugin as Guest>::invoke("request-headers".into(), "{}".into()).unwrap()).unwrap();
        assert_eq!(headers, json!([["Referer", "https://www.animegg.org"]]));
    }

    #[test]
    fn an_op_the_plug_in_does_not_answer_is_unknown() {
        assert_eq!(<Plugin as Guest>::invoke("pages".into(), "{}".into()), Err("unknown op: pages".to_owned()));
    }
}
