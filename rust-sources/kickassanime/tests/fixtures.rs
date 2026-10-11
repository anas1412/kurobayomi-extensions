//! Every operation over responses saved from kaa.lt and krussdomi.com on 2026-10-10 (tests/fixtures), no network.
//! The older-player crypto and the DASH split have no live sample (no server used the older player, and the one MPD
//! seen answered 404), so their inputs are made here: the AES vector by the openssl command line and the SHA-1 by
//! Python's hashlib, both independent of this crate.

use kuro_source::Filter;
use kuro_source::anime::{AnimeStatus, Video};
use kuro_src_kickassanime::api::{self, SearchRequest};
use kuro_src_kickassanime::extract;
use url::Url;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join(name))
        .expect("fixture")
}

fn text(name: &str) -> String {
    String::from_utf8(fixture(name)).unwrap()
}

#[test]
fn popular_reads_the_trending_page() {
    let page = api::parse_popular(&fixture("trending.json"), 1, false).unwrap();
    assert_eq!(page.animes.len(), 24);
    assert!(page.has_next_page, "page_count is 28");
    let first = &page.animes[0];
    assert_eq!(first.title, "Bungou Stray Dogs 5th Season");
    assert_eq!(first.url, "/bungo-stray-dogs-5-af88");
    assert_eq!(first.thumbnail_url.as_deref(), Some("https://kaa.lt/image/poster/bungo-stray-dogs-5-3e66-hq.webp"));
    assert!(!api::parse_popular(&fixture("trending.json"), 28, false).unwrap().has_next_page);
    assert_eq!(api::parse_popular(&fixture("trending.json"), 1, true).unwrap().animes[0].title, "Bungo Stray Dogs 5");
    assert_eq!(api::popular_url(2), "https://kaa.lt/api/show/trending?page=2");
}

#[test]
fn latest_reads_the_recent_page() {
    let page = api::parse_latest(&fixture("recent.json"), false).unwrap();
    assert_eq!(page.animes.len(), 36);
    assert!(page.has_next_page);
    assert_eq!(page.animes[0].url, "/ranma-12-2024-season-3-3a25");
    assert_eq!(api::latest_url(1), "https://kaa.lt/api/show/recent?type=all&page=1");
}

#[test]
fn text_search_posts_json_and_reads_max_page() {
    let request = api::search_request(1, "frieren", &api::SearchParams::default());
    assert_eq!(request, SearchRequest::Text { body: r#"{"page":1,"query":"frieren"}"#.into() });
    let page = api::parse_search(&fixture("search.json"), 1, false).unwrap();
    assert_eq!(
        page.animes.iter().map(|a| a.url.as_str()).collect::<Vec<_>>(),
        ["/sousou-no-frieren-2d15", "/sousou-no-frieren-no-mahou-561b", "/frieren-beyond-journeys-end-season-2-7dcd"]
    );
    assert!(!page.has_next_page, "maxPage is 1");
}

#[test]
fn filters_encode_to_the_parameter_the_site_answered() {
    let mut filters = api::filter_list();
    for filter in filters.0.iter_mut() {
        match filter {
            Filter::Group { name, state } if name == "Genre" => {
                if let Filter::CheckBox { state, .. } = &mut state[0] {
                    *state = true;
                }
            }
            Filter::Select { name, values, state } if name == "Year" => {
                *state = values.iter().position(|v| v == "2023").unwrap() as i32;
            }
            Filter::Select { name, state, .. } if name == "Type" => *state = 1,
            _ => {}
        }
    }
    let params = api::search_params(&filters);
    assert_eq!(params.filters_json(), r#"{"genres":["Action"],"year":2023,"type":"tv"}"#);
    // The exact URL fetched for anime_filtered.json.
    let SearchRequest::Browse { url } = api::search_request(1, "", &params) else { panic!("browse") };
    assert_eq!(
        url,
        "https://kaa.lt/api/anime?page=1&filters=eyJnZW5yZXMiOlsiQWN0aW9uIl0sInllYXIiOjIwMjMsInR5cGUiOiJ0diJ9"
    );
    let page = api::parse_search(&fixture("anime_filtered.json"), 1, false).unwrap();
    assert_eq!(page.animes.len(), 30);
    assert!(page.has_next_page, "maxPage is 3");
    assert_eq!(
        api::search_request(1, "", &api::SearchParams::default()),
        SearchRequest::Browse { url: "https://kaa.lt/api/anime?page=1".into() }
    );
}

#[test]
fn a_sub_page_overrides_the_query_and_is_parsed_by_its_path() {
    let params = api::SearchParams { sub_page: "show/recent".into(), ..Default::default() };
    assert_eq!(
        api::search_request(2, "x", &params),
        SearchRequest::SubPage("https://kaa.lt/api/show/recent?page=2".into())
    );
    assert_eq!(api::parse_sub_page("/api/show/recent", &fixture("recent.json"), 1, false).unwrap().animes.len(), 36);
    assert_eq!(
        api::parse_sub_page("/api/show/trending", &fixture("trending.json"), 1, false).unwrap().animes.len(),
        24
    );
    assert_eq!(api::parse_sub_page("/api/anime", &fixture("anime_filtered.json"), 1, false).unwrap().animes.len(), 30);
}

#[test]
fn a_slug_or_a_site_link_names_one_show() {
    assert_eq!(api::slug_of_query("slug:abc-1").unwrap().as_deref(), Some("abc-1"));
    assert_eq!(
        api::slug_of_query("https://kaa.lt/sousou-no-frieren-2d15/ep-1-f897b3").unwrap().as_deref(),
        Some("sousou-no-frieren-2d15")
    );
    assert!(api::slug_of_query("https://other.test/x").is_err());
    assert_eq!(api::slug_of_query("frieren").unwrap(), None);
}

#[test]
fn details_read_the_show_and_its_languages() {
    let languages = api::parse_languages(&fixture("language.json")).unwrap();
    assert_eq!(languages, ["ja-JP", "en-US", "es-419", "es-ES"]);
    let anime = api::parse_details(&fixture("show.json"), &languages, false).unwrap();
    assert_eq!(anime.title, "Sousou no Frieren");
    assert_eq!(anime.url, "/sousou-no-frieren-2d15");
    assert_eq!(anime.status, AnimeStatus::Completed);
    assert_eq!(anime.genre.as_deref(), Some("Adventure, Drama, Fantasy, Shounen"));
    let description = anime.description.unwrap();
    assert!(description.starts_with("During their decade-long quest"));
    assert!(description.contains("Available Dub Languages: Japanese, English, es-419, Spanish (España)\n"));
    assert!(description.contains("Season: Fall\n"));
    assert!(description.ends_with("Year: 2023"));
    assert_eq!(api::details_url("/x"), "https://kaa.lt/api/show/x");
}

#[test]
fn episodes_read_one_page_newest_last_in_site_order() {
    let page = api::parse_episode_page(&fixture("episodes.json")).unwrap();
    assert_eq!(page.page_count(), 1);
    let episodes = page.episodes("/sousou-no-frieren-2d15", "ja-JP");
    assert_eq!(episodes.len(), 28);
    assert_eq!(episodes[0].name, "Ep. 1 - The Journey's End");
    assert_eq!(episodes[0].url, "/sousou-no-frieren-2d15/ep-1-f897b3");
    assert_eq!(episodes[0].episode_number, 1.0);
    assert_eq!(episodes[0].scanlator.as_deref(), Some("Japanese"));
    assert_eq!(api::episodes_url("/s", 2, "en-US"), "https://kaa.lt/api/show/s/episodes?page=2&lang=en-US");
    let order = api::language_order(vec!["es-ES".into(), "en-US".into(), "ja-JP".into()], "ja-JP", "en-US");
    assert_eq!(order, ["ja-JP", "en-US", "es-ES"]);
}

#[test]
fn servers_are_read_from_the_episode() {
    assert_eq!(
        api::servers_url("/sousou-no-frieren-2d15/ep-1-f897b3"),
        "https://kaa.lt/api/show/sousou-no-frieren-2d15/episode/ep-1-f897b3"
    );
    let servers = api::parse_servers(&fixture("servers.json")).unwrap();
    assert_eq!(servers.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["VidStreaming", "BirdStream"]);
    assert!(servers[0].src.starts_with("https://krussdomi.com/cat-player/player?id=67d0c079169c31976b8d7970"));
}

#[test]
fn the_current_player_gives_the_hls_manifest_and_subtitles() {
    let page = extract::player_url(
        "https://krussdomi.com/cat-player/player?id=67d0c079169c31976b8d7970&source=vidstream&ln=ja-JP",
    )
    .unwrap();
    let data = extract::parse_new_player(&text("player_vid.html"), &page).unwrap();
    assert_eq!(data.manifest, "https://hls.krussdomi.com/manifest/67d0c079169c31976b8d7970/master.m3u8");
    assert_eq!(data.subtitles.len(), 9);
    assert_eq!(data.subtitles[0].lang, "English (en)");
    assert_eq!(
        data.subtitles[0].url,
        "https://subst.krussdomi.com/67d0c079169c31976b8d7970/67e0fa641967208e6c20882f.vtt"
    );
}

#[test]
fn the_bird_player_gives_a_dash_manifest_with_the_extra_slashes_removed() {
    let page = extract::player_url("https://krussdomi.com/cat-player/player?id=x&type=dash").unwrap();
    let data = extract::parse_new_player(&text("player_bird.html"), &page).unwrap();
    assert_eq!(data.manifest, "https://bl.krussdomi.com/mpd/12975044/master.mpd");
    assert_eq!(data.subtitles.len(), 5);
    assert_eq!(data.subtitles[1].lang, "English (en)");
}

#[test]
fn a_page_without_an_island_is_not_the_current_player() {
    let page = Url::parse("https://a.test/p").unwrap();
    assert_eq!(extract::parse_new_player("<html><body>cid: 'ab'</body></html>", &page), None);
}

#[test]
fn the_master_playlist_becomes_one_video_per_quality() {
    let player = Url::parse("https://krussdomi.com/cat-player/player?id=1").unwrap();
    let headers = extract::video_headers(&player);
    let master = "https://hls.krussdomi.com/manifest/67d0c079169c31976b8d7970/master.m3u8";
    let videos = extract::hls_videos("VidStreaming", master, &text("master.m3u8"), &[], &headers);
    let titles: Vec<&str> = videos.iter().map(|v| v.video_title.as_str()).collect();
    assert_eq!(
        titles,
        [
            "VidStreaming - 1080p (1920x1080) - 3.10 Mbps",
            "VidStreaming - 720p (1280x720) - 1.35 Mbps",
            "VidStreaming - 360p (640x360) - 479 kbps"
        ]
    );
    let best = &videos[0];
    assert_eq!(
        best.video_url,
        "https://hls.krussdomi.com/manifest/67d0c079169c31976b8d7970/67d0c5a1a60e80857fb0f89f/playlist.m3u8"
    );
    assert_eq!(best.resolution, Some(1080));
    assert_eq!(best.audio_tracks.len(), 16, "both rendition sets, duplicates by URL removed");
    assert_eq!(best.audio_tracks[0].lang, "Japanese", "the DEFAULT rendition comes first");
    let sent = &best.headers.as_ref().unwrap().0;
    assert!(sent.contains(&("Origin".into(), "https://krussdomi.com".into())));
    assert!(sent.iter().all(|(k, _)| k != "Referer"));
}

#[test]
fn a_media_playlist_is_one_video() {
    let videos = extract::hls_videos("S", "https://a.test/x.m3u8", "#EXTM3U\n#EXTINF:4,\nseg.ts\n", &[], &[]);
    assert_eq!(videos.len(), 1);
    assert_eq!((videos[0].video_url.as_str(), videos[0].video_title.as_str()), ("https://a.test/x.m3u8", "S - Video"));
}

#[test]
fn a_dash_manifest_becomes_one_video_per_representation() {
    let mpd = r#"<?xml version="1.0"?><MPD><Period><AdaptationSet>
        <Representation mimeType="video/mp4" width="1920" height="1080" bandwidth="3000000"><BaseURL>v1080.mp4</BaseURL></Representation>
        <Representation mimeType="video/mp4" width="1280" height="720" bandwidth="1500000"><BaseURL>https://c.test/v720.mp4</BaseURL></Representation>
        <Representation mimeType="audio/mp4" bandwidth="128000"><BaseURL>a.mp4</BaseURL></Representation>
        </AdaptationSet></Period></MPD>"#;
    let videos = extract::dash_videos("BirdStream", "https://bl.test/mpd/1/master.mpd", mpd, &[], &[]);
    assert_eq!(videos.len(), 2);
    assert_eq!(videos[0].video_title, "BirdStream - 1080p (1920x1080) - 3.00 Mbps");
    assert_eq!(videos[0].video_url, "https://bl.test/mpd/1/v1080.mp4");
    assert_eq!(videos[1].video_url, "https://c.test/v720.mp4");
    assert_eq!(videos[0].audio_tracks[0].url, "https://bl.test/mpd/1/a.mp4");
    assert_eq!(videos[0].audio_tracks[0].lang, "128 kbps");
}

#[test]
fn urls_from_players_are_normalised() {
    let base = Url::parse("https://krussdomi.com/cat-player/player").unwrap();
    assert_eq!(extract::fix_url("https:////bl.test/a", &base), "https://bl.test/a");
    assert_eq!(extract::fix_url("////bl.test/a", &base), "https://bl.test/a");
    assert_eq!(extract::fix_url("//bl.test/a", &base), "https://bl.test/a");
    assert_eq!(extract::fix_url("/subs/en.vtt", &base), "https://krussdomi.com/subs/en.vtt");
    assert_eq!(extract::fix_url(" https://x.test/a ", &base), "https://x.test/a");
    assert_eq!(
        extract::player_url("https://k.test/vast?id=1").unwrap().as_str(),
        "https://k.test/cat-player/player?id=1"
    );
}

#[test]
fn the_older_player_signs_its_source_call_with_sha1() {
    // cid is hex of "1.2.3.4|/player.php"; the expected digests are from Python hashlib.
    let html = "<script>var x = { cid: '312e322e332e347c2f706c617965722e706870' }</script>";
    let player = Url::parse("https://p.test/player.php?id=abc").unwrap();
    let url = extract::legacy_source_url(html, &player, "VidStreaming", "UA/1", 1000).unwrap();
    assert_eq!(url, "https://p.test/source.php?id=abc&e=1060&s=e1538821b47d39a37a55dbbc92c6625f5e898958");
    let bird = extract::legacy_source_url(html, &player, "BirdStream", "UA/1", 1000).unwrap();
    assert_eq!(bird, "https://p.test/source.php?id=abc&s=72c37bbc015945962208f93d3f8ca1b13327ebc6");
    assert_eq!(extract::legacy_source_url(html, &player, "CatStream", "UA/1", 1000), None);
    let duck = Url::parse("https://p.test/player.php?mid=m1").unwrap();
    assert!(
        extract::legacy_source_url(html, &duck, "DuckStream", "UA/1", 1000)
            .unwrap()
            .starts_with("https://p.test/source.php?mid=m1&e=1060&s=")
    );
}

#[test]
fn the_older_player_answer_is_decrypted_with_aes_256_cbc() {
    // openssl enc -aes-256-cbc -K hex("e13d...d3") -iv 000102..0f -base64 over the JSON below.
    let data = "bvpH1dcfT6xAC0pZt3eUA1Nlr+9JQVF/T07//jl6Ed6enZ8VzfjpqmnuJ8mDO/w2SpWje4OmKigOoAalIUDMkr9hu5B/VLhkkeDTS6dQnNI09ok20ljOQNE/DDSmr0TBrYeOs/qir/1FmKORdAuBS2ba39IHWw84w0KwXCY2W0M=";
    let body = format!(r#"{{"data":"{}:000102030405060708090a0b0c0d0e0f"}}"#, data.replace('/', "\\/"));
    let dto = extract::decrypt_source(&body, extract::legacy_key("VidStreaming").unwrap()).unwrap();
    assert_eq!(dto.hls, "https:////cdn.test/m/master.m3u8");
    let player = Url::parse("https://p.test/player.php").unwrap();
    let subs = extract::legacy_subtitles(&dto, &player);
    assert_eq!((subs[0].url.as_str(), subs[0].lang.as_str()), ("https://cdn.test/en.vtt", "English (en)"));
    assert!(extract::decrypt_source(&body, extract::legacy_key("BirdStream").unwrap()).is_err(), "a wrong key fails");
}

#[test]
fn videos_sort_preferred_server_then_quality_then_height() {
    let videos = vec![
        Video::new("1", "BirdStream - 1080p (1920x1080)"),
        Video::new("2", "VidStreaming - 360p (640x360)"),
        Video::new("3", "VidStreaming - 1080p (1920x1080)"),
        Video::new("4", "VidStreaming - 720p (1280x720)"),
    ];
    let urls: Vec<String> =
        extract::sort_videos(videos, "VidStreaming", "1080p").into_iter().map(|v| v.video_url).collect();
    assert_eq!(urls, ["3", "4", "2", "1"]);
}
