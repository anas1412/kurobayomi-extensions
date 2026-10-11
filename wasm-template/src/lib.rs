//! A source plug-in for Kurobayomi, ABI 1.4 (`wit/kuro-source.wit`). Copy this folder, rename the package, set the
//! identity below, and add the ops your site needs. An op this template does not answer returns the contract's
//! `Err("unknown op: <op>")`, which the app reads as "this source does not offer it".
//!
//! Build (see README.md): a core module, then wrap it as a component with `wasm-tools`.
//!
//! To call the site, use the host's `http` import with a JSON request, for example
//! `{"method":"GET","url":"https://example.org/list"}`. The answer is JSON with `status`, `headers` and `body`.

wit_bindgen::generate!({
    path: "wit",
    world: "source",
});

/// The identity of the source. Keep the id for good: backups and the library refer to it. The id must be unique among
/// the sources of one package.
const ID: i64 = 1;
const NAME: &str = "Example source";
const LANG: &str = "en";
const SUPPORTS_LATEST: bool = false;

struct Plugin;

fn descriptor() -> serde_json::Value {
    serde_json::json!({
        "id": ID,
        "name": NAME,
        "lang": LANG,
        "supportsLatest": SUPPORTS_LATEST,
    })
}

impl Guest for Plugin {
    fn invoke(op: String, _input_json: String) -> Result<String, String> {
        match op.as_str() {
            "info" => Ok(descriptor().to_string()),
            "create-sources" => Ok(serde_json::Value::Array(vec![descriptor()]).to_string()),
            _ => Err(format!("unknown op: {op}")),
        }
    }
}

export!(Plugin);
