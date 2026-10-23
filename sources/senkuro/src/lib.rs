//! Senkuro (<https://senkuro.com>), a Russian manga source plug-in for Kurobayomi, ABI 1.5 (`wit/kuro-source.wit`).
//! Ported from the keiyoushi `ru/senkuro` extension (Apache-2.0). This file is the ABI glue: the ops and the one network
//! path (the `http` import), which picks the mirror that answers. The queries and parsers are in `api.rs`.

mod api;

use serde::Deserialize;
use serde_json::{Value, json};

wit_bindgen::generate!({
    path: "wit",
    world: "source",
});

use api::Res;
use kuro::source::host;

/// The source id: `generate_id("Senkuro", "ru", 1)`, the MD5 of `senkuro/ru/1`, as the Kotlin extension has it. Keep it:
/// backups and the library name it.
const ID: i64 = 1452500215874655811;
const NAME: &str = "Senkuro";
const LANG: &str = "ru";
/// The Kotlin keeps to 3 requests a second (`rateLimit(3)`).
const REQUEST_GAP_MS: u64 = 350;
/// The mirror that answered last, tried first next time.
const MIRROR_PREF: &str = "mirror";

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
#[serde(rename_all = "camelCase")]
struct Envelope {
    #[serde(default)]
    args: Args,
    /// The app's "Show adult (18+) titles" switch. An app that does not send it hides them.
    #[serde(default)]
    show_adult: bool,
}

#[derive(Deserialize)]
struct HttpResponse {
    status: u16,
    body: String,
}

fn descriptor() -> Value {
    json!({ "id": ID, "name": NAME, "lang": LANG, "supportsLatest": false, "requestGapMs": REQUEST_GAP_MS })
}

/// Posts one GraphQL request to one mirror. Anything but a JSON answer (a refused address, DDoS-Guard's geo-block page,
/// an HTTP error) is an error.
fn post(base: &str, body: &str) -> Res<String> {
    let request = json!({
        "method": "POST",
        "url": api::api_url(base),
        "headers": [
            ["Content-Type", "application/json"],
            ["User-Agent", "Tachiyomi (+https://github.com/keiyoushi/extensions-source)"],
            ["App-Id", "4026531840100"],
            ["App-Version", "060626"],
        ],
        "body": body,
    });
    let response: HttpResponse = serde_json::from_str(&host::http(&request.to_string())?).map_err(|e| format!("invalid http reply: {e}"))?;
    match response.status {
        200..=299 if response.body.trim_start().starts_with('{') => Ok(response.body),
        200..=299 => Err(format!("{base} sent no JSON (blocked from this network?)")),
        429 => Err("Senkuro is limiting requests; wait a few seconds and try again".into()),
        status => Err(format!("HTTP {status} from {base}")),
    }
}

/// Sends a GraphQL request to the mirror that answered last, then to the other one, and keeps the one that answers.
fn send(body: &str) -> Res<String> {
    let kept = host::pref_get(MIRROR_PREF);
    let mut mirrors = api::MIRRORS;
    if serde_json::from_str::<String>(&kept).is_ok_and(|m| m == mirrors[1]) {
        mirrors.reverse();
    }
    let mut errors = Vec::new();
    for (i, base) in mirrors.iter().enumerate() {
        match post(base, body) {
            Ok(answer) => {
                if i > 0 && host::pref_set(MIRROR_PREF, &json!(base).to_string()).is_err() {
                    host::log(2, "the Senkuro mirror could not be kept");
                }
                return Ok(answer);
            }
            Err(error) => errors.push(error),
        }
    }
    Err(errors.join("; "))
}

fn to_value<T: serde::Serialize>(value: T) -> Res<Value> {
    serde_json::to_value(value).map_err(|e| e.to_string())
}

fn run(op: &str, args: Args, show_adult: bool) -> Res<Value> {
    match op {
        // the Kotlin's Popular is the search sorted by popularity, which search keeps
        "popular" => to_value(api::parse_search(&send(&api::search_body(args.page, "", show_adult))?)?),
        "search" => to_value(api::parse_search(&send(&api::search_body(args.page, &args.query, show_adult))?)?),
        "details" => {
            let manga = args.manga.ok_or("details needs args.manga")?;
            to_value(api::parse_details(&send(&api::details_body(&manga.url))?)?)
        }
        "chapters" => {
            let manga = args.manga.ok_or("chapters needs args.manga")?;
            to_value(api::parse_chapters(&manga.url, &send(&api::chapters_body(&manga.url))?)?)
        }
        "pages" => {
            let chapter = args.chapter.ok_or("pages needs args.chapter")?;
            let images = api::parse_pages(&send(&api::pages_body(&chapter.url)?)?)?;
            to_value(images.into_iter().map(|image| json!({ "url": "", "imageUrl": image })).collect::<Vec<_>>())
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
            "popular" | "search" | "details" | "chapters" | "pages" => {
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
    fn info_carries_the_id_and_the_spacing() {
        let info: Value = serde_json::from_str(&<Plugin as Guest>::invoke("info".into(), "{}".into()).unwrap()).unwrap();
        assert_eq!((info["id"].as_i64(), info["requestGapMs"].as_u64()), (Some(ID), Some(350)));
        assert_eq!((info["lang"].as_str(), info["supportsLatest"].as_bool()), (Some("ru"), Some(false)));
    }

    #[test]
    fn an_op_the_plug_in_does_not_answer_is_unknown() {
        assert_eq!(<Plugin as Guest>::invoke("latest".into(), "{}".into()), Err("unknown op: latest".to_owned()));
    }
}
