//! `cargo run -p kuro-src-comix --example live_check`
//!
//! Runs popular, latest, search, details, chapters and pages once against comix.to, one request at a time and one
//! second apart, and prints what each returned. Exits non-zero when a step fails or returns nothing.

use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use kuro_source::{CatalogueSource, FilterList, MangaSource};
use kuro_src_comix::{BASE_URL, Comix};

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let dir = tempfile::tempdir().expect("scratch folder");
    let prefs = kuro_prefs::network::NetworkPrefs::new(kuro_prefs::store::PrefStore::in_memory());
    let (toaster, _feed) = kuro_platform::toast::toast_channel();
    let client = match kuro_net::NetworkClient::new(
        &prefs,
        dir.path().join("cookies.json"),
        Arc::new(kuro_platform::webview::NoWebView),
        toaster,
    ) {
        Ok(client) => client,
        Err(e) => {
            eprintln!("FAIL network client: {e}");
            return ExitCode::FAILURE;
        }
    };
    let source = Comix::new(client, BASE_URL, Duration::from_secs(1));
    let mut failed = false;
    let mut report = |step: &str, outcome: Result<String, String>| match outcome {
        Ok(line) => println!("ok   {step}: {line}"),
        Err(line) => {
            failed = true;
            println!("FAIL {step}: {line}");
        }
    };
    let nonempty = |n: usize, what: String| {
        if n == 0 {
            Err(format!("nothing returned ({what})"))
        } else {
            Ok(what)
        }
    };

    let popular = source.popular(1).await;
    report(
        "popular",
        popular.as_ref().map_err(|e| e.to_string()).and_then(|p| {
            nonempty(
                p.mangas.len(),
                format!(
                    "{} mangas, next={}, first={:?}",
                    p.mangas.len(),
                    p.has_next_page,
                    p.mangas.first().map(|m| &m.title)
                ),
            )
        }),
    );
    let latest = source.latest(1).await;
    report(
        "latest",
        latest.as_ref().map_err(|e| e.to_string()).and_then(|p| {
            nonempty(
                p.mangas.len(),
                format!(
                    "{} mangas, first={:?}",
                    p.mangas.len(),
                    p.mangas.first().map(|m| &m.title)
                ),
            )
        }),
    );
    let search = source.search(1, "villains", &FilterList::default()).await;
    report(
        "search",
        search.as_ref().map_err(|e| e.to_string()).and_then(|p| {
            nonempty(
                p.mangas.len(),
                format!(
                    "{} mangas for \"villains\", first={:?}",
                    p.mangas.len(),
                    p.mangas.first().map(|m| (&m.title, &m.url))
                ),
            )
        }),
    );

    // The first search hit with chapters: details, then its chapters, then the pages of the newest chapter.
    let Some(manga) = search.ok().and_then(|p| p.mangas.into_iter().next()) else {
        println!("FAIL details/chapters/pages: no manga to open");
        return ExitCode::FAILURE;
    };
    match source.get_manga_details(&manga).await {
        Ok(d) => report(
            "details",
            Ok(format!(
                "{:?} by {:?}, status {}, genres {:?}",
                d.title, d.author, d.status, d.genre
            )),
        ),
        Err(e) => report("details", Err(e.to_string())),
    }
    let chapters = source.get_chapter_list(&manga).await;
    report(
        "chapters",
        chapters.as_ref().map_err(|e| e.to_string()).and_then(|c| {
            nonempty(
                c.len(),
                format!(
                    "{} chapters, newest={:?}",
                    c.len(),
                    c.first().map(|c| (&c.name, &c.url, &c.scanlator))
                ),
            )
        }),
    );
    if let Some(chapter) = chapters.ok().and_then(|c| c.into_iter().next()) {
        let pages = source.get_page_list(&chapter).await;
        report(
            "pages",
            pages.as_ref().map_err(|e| e.to_string()).and_then(|p| {
                nonempty(
                    p.len(),
                    format!(
                        "{} pages, first={:?}",
                        p.len(),
                        p.first().and_then(|p| p.image_url.as_deref())
                    ),
                )
            }),
        );
    } else {
        report("pages", Err("no chapter to open".into()));
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
