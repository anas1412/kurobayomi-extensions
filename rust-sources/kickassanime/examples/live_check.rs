//! `cargo run -p kuro-src-kickassanime --example live_check [query]`
//!
//! Runs popular, latest, search, details, episodes, hosters and videos against the real site once, one request at a
//! time and one second apart. Prints a summary and exits non-zero when a step errors or returns nothing. No video or
//! image file is downloaded: only the API, the player pages and the playlists (text).

use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use kuro_net::NetworkClient;
use kuro_source::FilterList;
use kuro_source::anime::AnimeSource;

fn client(dir: &std::path::Path) -> Result<NetworkClient, String> {
    let prefs = kuro_prefs::network::NetworkPrefs::new(kuro_prefs::store::PrefStore::in_memory());
    let (toaster, _feed) = kuro_platform::toast::toast_channel();
    NetworkClient::new(&prefs, dir.join("cookies.json"), Arc::new(kuro_platform::webview::NoWebView), toaster)
        .map_err(|e| e.to_string())
}

fn step<T>(
    name: &str,
    result: Result<T, kuro_source::SourceError>,
    empty: impl Fn(&T) -> bool,
    failed: &mut bool,
) -> Option<T> {
    match result {
        Ok(value) if empty(&value) => {
            println!("FAIL {name}: returned nothing");
            *failed = true;
            None
        }
        Ok(value) => Some(value),
        Err(err) => {
            println!("FAIL {name}: {err}");
            *failed = true;
            None
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let query = std::env::args().nth(1).unwrap_or_else(|| "frieren".into());
    let scratch = tempfile::tempdir().expect("scratch folder");
    let client = match client(scratch.path()) {
        Ok(client) => client,
        Err(e) => {
            println!("FAIL network client: {e}");
            return ExitCode::FAILURE;
        }
    };
    let source = kuro_src_kickassanime::with_interval(client, Duration::from_secs(1));
    let mut failed = false;
    println!("{} ({}) id {}", source.name(), source.lang(), source.id());

    if let Some(page) = step("popular", source.popular(1).await, |p| p.animes.is_empty(), &mut failed) {
        println!(
            "ok   popular: {} animes, next {}; first {:?} {}",
            page.animes.len(),
            page.has_next_page,
            page.animes[0].title,
            page.animes[0].url
        );
    }
    if let Some(page) = step("latest", source.latest(1).await, |p| p.animes.is_empty(), &mut failed) {
        println!(
            "ok   latest: {} animes, next {}; first {:?}",
            page.animes.len(),
            page.has_next_page,
            page.animes[0].title
        );
    }
    let Some(found) =
        step("search", source.search(1, &query, &FilterList::default()).await, |p| p.animes.is_empty(), &mut failed)
    else {
        return ExitCode::FAILURE;
    };
    println!(
        "ok   search {query:?}: {} animes; first {:?} {}",
        found.animes.len(),
        found.animes[0].title,
        found.animes[0].url
    );
    let anime = found.animes[0].clone();
    if let Some(details) = step("details", source.anime_details(&anime).await, |a| a.title.is_empty(), &mut failed) {
        let first_line = details
            .description
            .as_deref()
            .unwrap_or_default()
            .lines()
            .find(|l| l.starts_with("Available"))
            .unwrap_or_default()
            .to_owned();
        println!(
            "ok   details: {:?}, status {:?}, genres {:?}; {first_line}",
            details.title,
            details.status,
            details.genre.unwrap_or_default()
        );
    }
    let Some(episodes) = step("episodes", source.episode_list(&anime).await, Vec::is_empty, &mut failed) else {
        return ExitCode::FAILURE;
    };
    let first = episodes.last().expect("not empty").clone();
    println!(
        "ok   episodes: {}; newest {:?}, oldest {:?} {} [{}]",
        episodes.len(),
        episodes[0].name,
        first.name,
        first.url,
        first.scanlator.clone().unwrap_or_default()
    );
    let Some(hosters) = step("hosters", source.hosters(&first).await, Vec::is_empty, &mut failed) else {
        return ExitCode::FAILURE;
    };
    let hosters = source.sort_hosters(hosters);
    println!("ok   hosters: {}", hosters.iter().map(|h| h.hoster_name.as_str()).collect::<Vec<_>>().join(", "));
    let mut any_video = false;
    for hoster in &hosters {
        match source.videos_for_hoster(hoster).await {
            Ok(videos) if !videos.is_empty() => {
                any_video = true;
                let v = &videos[0];
                println!("ok   videos {}: {} videos; best {:?}", hoster.hoster_name, videos.len(), v.video_title);
                println!("       url {}", v.video_url);
                println!(
                    "       {} subtitles (first {:?}), {} audio tracks (first {:?})",
                    v.subtitle_tracks.len(),
                    v.subtitle_tracks.first().map(|t| &t.lang),
                    v.audio_tracks.len(),
                    v.audio_tracks.first().map(|t| &t.lang)
                );
                println!(
                    "       headers {:?}",
                    v.headers.as_ref().map(|h| h.0.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>())
                );
            }
            Ok(_) => println!("warn videos {}: none (manifest unreachable or unknown player)", hoster.hoster_name),
            Err(err) => println!("warn videos {}: {err}", hoster.hoster_name),
        }
    }
    if !any_video {
        println!("FAIL videos: no hoster gave a video");
        failed = true;
    }
    println!("{}", if failed { "RESULT: FAILED" } else { "RESULT: OK" });
    if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}
