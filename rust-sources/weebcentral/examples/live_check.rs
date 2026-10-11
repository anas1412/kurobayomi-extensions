//! `cargo run -p kuro-src-weebcentral --example live_check`
//!
//! Runs popular, latest, search, details, chapters and pages against the real site once, one request at a time (the
//! source waits two seconds between requests). Exits non-zero when a step fails or returns nothing.

use std::process::ExitCode;
use std::sync::Arc;

use kuro_net::NetworkClient;
use kuro_source::model::{MangasPage, Page, SChapter, SManga};
use kuro_source::{CatalogueSource, FilterList, MangaSource, SourceError};

fn client(dir: &std::path::Path) -> Result<NetworkClient, String> {
    let prefs = kuro_prefs::network::NetworkPrefs::new(kuro_prefs::store::PrefStore::in_memory());
    let (toaster, _feed) = kuro_platform::toast::toast_channel();
    NetworkClient::new(&prefs, dir.join("cookies.json"), Arc::new(kuro_platform::webview::NoWebView), toaster).map_err(|e| e.to_string())
}

fn show(list: &[SManga]) -> String {
    list.iter().take(3).map(|m| format!("{:?} {}", m.title, m.url)).collect::<Vec<_>>().join(" | ")
}

/// Prints one step. `None` when it failed or returned nothing, so the caller stops.
fn step<T>(name: &str, result: Result<T, SourceError>, count: impl Fn(&T) -> usize, summary: impl Fn(&T) -> String) -> Option<T> {
    match result {
        Ok(value) if count(&value) > 0 => {
            println!("ok   {name}: {} item(s); {}", count(&value), summary(&value));
            Some(value)
        }
        Ok(_) => {
            println!("FAIL {name}: returned nothing");
            None
        }
        Err(e) => {
            println!("FAIL {name}: {e}");
            None
        }
    }
}

fn listing(p: &MangasPage) -> String {
    format!("next={} {}", p.has_next_page, show(&p.mangas))
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let scratch = tempfile::tempdir().expect("scratch folder");
    let source = match client(scratch.path()) {
        Ok(c) => kuro_src_weebcentral::source(c),
        Err(e) => {
            println!("FAIL network client: {e}");
            return ExitCode::FAILURE;
        }
    };
    println!("source {} ({}) id {}", source.name(), source.lang(), source.id());
    let count = |p: &MangasPage| p.mangas.len();
    let Some(_) = step("popular", source.popular(1).await, count, listing) else { return ExitCode::FAILURE };
    let Some(_) = step("latest", source.latest(1).await, count, listing) else { return ExitCode::FAILURE };
    let Some(found) = step("search \"one piece\"", source.search(1, "one piece", &FilterList::default()).await, count, listing) else {
        return ExitCode::FAILURE;
    };
    let details = step("details", source.get_manga_details(&found.mangas[0]).await, |m| usize::from(!m.title.is_empty()), |m| {
        format!(
            "title={:?} author={:?} status={} genre={:?} cover={:?} description={} chars",
            m.title, m.author, m.status, m.genre, m.thumbnail_url, m.description.as_deref().map_or(0, str::len)
        )
    });
    let Some(details) = details else { return ExitCode::FAILURE };
    let chapters = step("chapters", source.get_chapter_list(&details).await, Vec::len, |c: &Vec<SChapter>| {
        format!("first={:?} {} date={} scanlator={:?}; last={:?}", c[0].name, c[0].url, c[0].date_upload, c[0].scanlator, c[c.len() - 1].name)
    });
    let Some(chapters) = chapters else { return ExitCode::FAILURE };
    let pages = step("pages", source.get_page_list(&chapters[0]).await, Vec::len, |p: &Vec<Page>| {
        format!("first={:?} last={:?}", p[0].image_url, p[p.len() - 1].image_url)
    });
    if pages.is_none() {
        return ExitCode::FAILURE;
    }
    println!("all steps returned data");
    ExitCode::SUCCESS
}
