//! InkStory (<https://inkstory.net>), a Russian manga source plug-in for Kurobayomi, ABI 1.5 (`wit/kuro-source.wit`).
//! Ported from the keiyoushi `ru/inkstory` extension and its `inkstory` theme (Apache-2.0). This file is the ABI glue:
//! the ops, the one network path (the `http` import) and the page image fix-up. The URLs and parsers are in `api.rs`.

mod api;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::Deserialize;
use serde_json::{Value, json};

wit_bindgen::generate!({
    path: "wit",
    world: "source",
});

use api::Res;
use kuro::source::host;

/// The source id: `generate_id("InkStory", "ru", 2)`, the MD5 of `inkstory/ru/2` (the Kotlin's `versionId = 2`). Keep
/// it: backups and the library name it.
const ID: i64 = 2026387314200968454;
const NAME: &str = "InkStory";
const LANG: &str = "ru";

/// The `args` of an op. Each op reads only the fields it needs.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Args {
    page: i32,
    query: String,
    manga: Option<Ref>,
    chapter: Option<Ref>,
    /// `unscramble`: the fetched image, base64.
    body: String,
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
    #[serde(default)]
    headers: Vec<(String, String)>,
    body: String,
    #[serde(default)]
    body_encoding: Option<String>,
}

fn descriptor() -> Value {
    json!({ "id": ID, "name": NAME, "lang": LANG, "supportsLatest": true })
}

/// GETs an API address through the host, which refuses any host outside `hosts` in `manifest.toml`. The headers are the
/// Kotlin's: the API wants its own User-Agent and the service name. A non-2xx answer is an error.
fn get(url: &str) -> Res<HttpResponse> {
    let headers = [
        ["User-Agent", "Tachiyomi (+https://github.com/keiyoushi/extensions-source)"],
        ["Accept", "application/json, text/plain, */*"],
        ["X-Service-Name", api::SERVICE],
    ];
    let request = json!({ "method": "GET", "url": url, "headers": headers });
    let response: HttpResponse = serde_json::from_str(&host::http(&request.to_string())?).map_err(|e| format!("invalid http reply: {e}"))?;
    if response.body_encoding.is_some() {
        return Err(format!("{url} sent a binary body"));
    }
    match response.status {
        200..=299 => Ok(response),
        429 => Err("InkStory is limiting requests; wait a few seconds and try again".into()),
        status => Err(format!("HTTP {status} from InkStory")),
    }
}

fn to_value<T: serde::Serialize>(value: T) -> Res<Value> {
    serde_json::to_value(value).map_err(|e| e.to_string())
}

fn books(page: i32, query: &str) -> Res<Value> {
    let response = get(&api::books_url(page, query))?;
    let total = response.headers.iter().find(|(name, _)| name.eq_ignore_ascii_case("x-estimated-total-hits")).and_then(|(_, v)| v.trim().parse().ok());
    to_value(api::parse_books(&response.body, page, total)?)
}

/// The chapter list needs the book id, which the slug in the manga url does not carry: the book is read first.
fn chapters(slug: &str) -> Res<Value> {
    let id = api::book_id(&get(&api::details_url(slug))?.body)?;
    let branches = get(&api::branches_url(&id))?.body;
    to_value(api::parse_chapters(&branches, &get(&api::chapters_url(&id))?.body)?)
}

/// ABI 1.5 image hook: XOR-ciphered page images read back; anything else shown as fetched.
fn unscramble(body: &str) -> Res<Value> {
    let bytes = STANDARD.decode(body).map_err(|e| format!("body is not base64: {e}"))?;
    Ok(json!({ "body": api::unxor(&bytes).map(|plain| STANDARD.encode(plain)) }))
}

fn run(op: &str, args: Args) -> Res<Value> {
    match op {
        "popular" => books(args.page, ""),
        "search" => books(args.page, &args.query),
        "latest" => to_value(api::parse_feed(&get(&api::feed_url(args.page))?.body)?),
        "details" => {
            let manga = args.manga.ok_or("details needs args.manga")?;
            to_value(api::parse_details(&get(&api::details_url(&manga.url))?.body)?)
        }
        "chapters" => chapters(&args.manga.ok_or("chapters needs args.manga")?.url),
        "pages" => {
            let chapter = args.chapter.ok_or("pages needs args.chapter")?;
            let images = api::parse_pages(&get(&api::pages_url(&chapter.url))?.body)?;
            to_value(images.into_iter().map(|image| json!({ "url": "", "imageUrl": image })).collect::<Vec<_>>())
        }
        "unscramble" => unscramble(&args.body),
        _ => Err(format!("unknown op: {op}")),
    }
}

struct Plugin;

impl Guest for Plugin {
    fn invoke(op: String, input_json: String) -> Result<String, String> {
        let out = match op.as_str() {
            "info" => descriptor(),
            "create-sources" => Value::Array(vec![descriptor()]),
            "popular" | "latest" | "search" | "details" | "chapters" | "pages" | "unscramble" => {
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
        assert_eq!((info["id"].as_i64(), info["lang"].as_str()), (Some(ID), Some("ru")));
    }

    #[test]
    fn the_unscramble_op_reads_a_xor_page_and_leaves_a_plain_one() {
        let ciphered = STANDARD.encode(include_bytes!("../fixtures/page-start.bin"));
        let out: Value = serde_json::from_str(&<Plugin as Guest>::invoke("unscramble".into(), json!({"args": {"body": ciphered, "headers": []}}).to_string()).unwrap()).unwrap();
        let plain = STANDARD.decode(out["body"].as_str().unwrap()).unwrap();
        assert!(plain.starts_with(&[0xFF, 0xD8, 0xFF]));
        let again = json!({"args": {"body": STANDARD.encode(&plain), "headers": []}}).to_string();
        assert_eq!(<Plugin as Guest>::invoke("unscramble".into(), again), Ok(r#"{"body":null}"#.to_owned()));
    }

    #[test]
    fn an_op_the_plug_in_does_not_answer_is_unknown() {
        assert_eq!(<Plugin as Guest>::invoke("hosters".into(), "{}".into()), Err("unknown op: hosters".to_owned()));
    }
}
