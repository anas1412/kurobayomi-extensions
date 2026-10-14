//! MangaDex (<https://mangadex.org>), a manga source plug-in for Kurobayomi, ABI 1.5 (`wit/kuro-source.wit`). Ported from
//! the keiyoushi `all/mangadex` extension (Apache-2.0), English only. This file is the ABI glue: the ops and the one
//! network path, the `http` import. The URLs and parsers are in `api.rs`.

mod api;

use serde::Deserialize;
use serde_json::{Value, json};

wit_bindgen::generate!({
    path: "wit",
    world: "source",
});

use api::Res;
use kuro::source::host;

/// The source id the app derives for it: `generate_id("MangaDex", "en", 1)`, the MD5 of `mangadex/en/1`, as the Kotlin
/// extension has it. Keep it: backups and the library name it.
const ID: i64 = 2499283573021220255;
const NAME: &str = "MangaDex";
const LANG: &str = "en";
/// The API allows about 5 requests a second; the Kotlin keeps to 3 (`rateLimit(3)`).
const REQUEST_GAP_MS: u64 = 350;

/// The `args` of an op. Each op reads only the fields it needs.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Args {
    page: i32,
    query: String,
    manga: Option<Ref>,
    chapter: Option<Ref>,
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

/// Fetches an API address through the host, which refuses any host outside `hosts` in `manifest.toml`. A non-2xx answer
/// is an error: the API's own message when it gives one.
fn get(url: &str) -> Res<String> {
    let request = json!({ "method": "GET", "url": url, "headers": [["Accept", "application/json"]] });
    let response: HttpResponse = serde_json::from_str(&host::http(&request.to_string())?).map_err(|e| format!("invalid http reply: {e}"))?;
    if response.body_encoding.is_some() {
        return Err(format!("{url} sent a binary body"));
    }
    match response.status {
        200..=299 => Ok(response.body),
        429 => Err("MangaDex is limiting requests; wait a few seconds and try again".into()),
        status => Err(api::error_message(&response.body).unwrap_or_else(|| format!("HTTP {status} from MangaDex"))),
    }
}

fn to_value<T: serde::Serialize>(value: T) -> Res<Value> {
    serde_json::to_value(value).map_err(|e| e.to_string())
}

fn popular(page: i32) -> Res<Value> {
    to_value(api::parse_mangas(&get(&api::popular_url(page, &api::thirty_days_before(host::unix_seconds())))?)?)
}

/// The newest chapters name the manga; their titles and covers come from one more request.
fn latest(page: i32) -> Res<Value> {
    let (ids, more) = api::parse_latest_ids(&get(&api::latest_url(page))?)?;
    if ids.is_empty() {
        return to_value(api::MangasPage { mangas: Vec::new(), has_next_page: false });
    }
    to_value(api::parse_in_order(&get(&api::by_ids_url(&ids))?, &ids, more)?)
}

/// Every page of the chapter feed, newest first.
fn chapters(manga: &str) -> Res<Value> {
    let mut all = Vec::new();
    let mut offset = 0;
    loop {
        let page = api::parse_feed(&get(&api::feed_url(manga, offset)?)?)?;
        all.extend(page.chapters);
        match page.next_offset {
            Some(next) => offset = next,
            None => return to_value(all),
        }
    }
}

fn run(op: &str, args: Args) -> Res<Value> {
    match op {
        "popular" => popular(args.page),
        "latest" => latest(args.page),
        "search" => to_value(api::parse_mangas(&get(&api::search_url(args.page, &args.query))?)?),
        "details" => {
            let manga = args.manga.ok_or("details needs args.manga")?;
            to_value(api::parse_details(&get(&api::details_url(&manga.url)?)?)?)
        }
        "chapters" => chapters(&args.manga.ok_or("chapters needs args.manga")?.url),
        "pages" => {
            let chapter = args.chapter.ok_or("pages needs args.chapter")?;
            let pages = api::parse_pages(&get(&api::at_home_url(&chapter.url)?)?)?;
            to_value(pages.into_iter().map(|image| json!({ "url": "", "imageUrl": image })).collect::<Vec<_>>())
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
            // ABI 1.5: the page-image headers. They replace the app's default set for this source.
            "request-headers" => json!([["Accept", "image/avif,image/webp,*/*"]]),
            "popular" | "latest" | "search" | "details" | "chapters" | "pages" => {
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
    fn info_carries_the_id_and_the_spacing() {
        let info: Value = serde_json::from_str(&<Plugin as Guest>::invoke("info".into(), "{}".into()).unwrap()).unwrap();
        assert_eq!((info["id"].as_i64(), info["requestGapMs"].as_u64()), (Some(ID), Some(350)));
        assert_eq!(info["supportsLatest"], true);
    }

    #[test]
    fn an_op_the_plug_in_does_not_answer_is_unknown() {
        assert_eq!(<Plugin as Guest>::invoke("hosters".into(), "{}".into()), Err("unknown op: hosters".to_owned()));
    }
}
