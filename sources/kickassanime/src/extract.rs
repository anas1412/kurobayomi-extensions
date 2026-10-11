//! The video extractor as pure functions (`extractors/KickAssAnimeExtractor.kt` and the parts of `PlaylistUtils.kt`
//! it uses). Each server's `src` is a player page: an Astro island whose `props` carry the manifest (HLS or DASH) and
//! the subtitle tracks. The manifest is then split per quality: [`hls_videos`] for a master playlist, [`dash_videos`]
//! for an MPD.
//!
//! Not ported: the older `cid` player (SHA-1 signed `source.php` call and AES-256-CBC answer). Every server seen on
//! 2026-10-10 used the current player.

use std::cmp::Reverse;

use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

/// The user agent the Kotlin extractor sends with every stream request.
pub const VIDEO_USER_AGENT: &str =
    "Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/129.0.0.0 Mobile Safari/537.36";

/// A subtitle or audio track: where it is and its label.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub url: String,
    #[serde(default)]
    pub lang: String,
}

/// One playable video, as the app reads it. `headers` are sent with the playlist, the segments and the subtitles.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Video {
    pub video_url: String,
    pub video_title: String,
    pub resolution: Option<i32>,
    pub bitrate: Option<i32>,
    pub headers: Vec<(String, String)>,
    pub subtitle_tracks: Vec<Track>,
    pub audio_tracks: Vec<Track>,
}

/// What the current player page gives: the manifest address and the subtitle tracks.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerData {
    pub manifest: String,
    pub subtitles: Vec<Track>,
}

/// The player page of a server `src`: an ad (`/vast`) address is turned into the player itself.
pub fn player_url(src: &str) -> Result<Url, String> {
    let mut url = Url::parse(src).map_err(|e| e.to_string())?;
    if src.contains("/vast") {
        url.set_path("/cat-player/player");
    }
    Ok(url)
}

/// The headers of a video request of this source, except the Origin: no Referer, a fixed mobile Chrome user agent. The
/// ABI 1.5 `request-headers` answers these (the Origin depends on the player's host, so it is not in that list).
pub fn common_headers() -> Vec<(String, String)> {
    [
        ("Accept", "*/*"),
        ("Accept-Language", "en-US,en;q=0.9"),
        ("Sec-Fetch-Dest", "empty"),
        ("Sec-Fetch-Mode", "cors"),
        ("Sec-Fetch-Site", "same-site"),
        ("User-Agent", VIDEO_USER_AGENT),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .collect()
}

/// The headers the player must send with the playlist, the segments and the subtitles: [`common_headers`] and the
/// Origin of the player page.
pub fn video_headers(player: &Url) -> Vec<(String, String)> {
    let host = player.host_str().unwrap_or_default();
    let mut headers = common_headers();
    headers.push(("Origin".to_owned(), format!("https://{host}")));
    headers
}

/// Normalises a URL the players give (Kotlin `fixUrl`): `https:////host/x` and `////host/x` become `https://host/x`,
/// `//host/x` gets `https:`, `/x` takes the scheme and host of `base`, anything else is kept.
pub fn fix_url(raw: &str, base: &Url) -> String {
    let trimmed = raw.trim();
    for scheme in ["https:", "http:"] {
        if let Some(rest) = trimmed.strip_prefix(scheme)
            && rest.starts_with("//")
        {
            return format!("{scheme}//{}", rest.trim_start_matches('/'));
        }
    }
    if trimmed.starts_with("//") {
        format!("https://{}", trimmed.trim_start_matches('/'))
    } else if trimmed.starts_with('/') {
        format!("{}://{}{trimmed}", base.scheme(), base.host_str().unwrap_or_default())
    } else {
        trimmed.to_owned()
    }
}

/// Astro serialises island props as `[0, value]` for a plain value (objects hold encoded fields) and `[1, [...]]` for
/// an array of encoded values. Other tags (dates, maps, ...) are kept as they are.
fn astro_decode(value: &Value) -> Value {
    match value {
        Value::Array(pair) if pair.len() <= 2 && pair.first().and_then(Value::as_i64).is_some() => {
            let inner = pair.get(1).cloned().unwrap_or(Value::Null);
            match (pair[0].as_i64(), inner) {
                (Some(0), Value::Object(map)) => {
                    Value::Object(map.iter().map(|(k, v)| (k.clone(), astro_decode(v))).collect())
                }
                (Some(1), Value::Array(items)) => Value::Array(items.iter().map(astro_decode).collect()),
                (_, other) => other,
            }
        }
        Value::Object(map) => Value::Object(map.iter().map(|(k, v)| (k.clone(), astro_decode(v))).collect()),
        other => other.clone(),
    }
}

/// The manifest and subtitles of the current player, or `None` when the page has no island with a manifest.
pub fn parse_new_player(html: &str, page: &Url) -> Option<PlayerData> {
    let doc = Html::parse_document(html);
    let islands = Selector::parse("astro-island[props]").ok()?;
    doc.select(&islands).find_map(|island| {
        let raw: Value = serde_json::from_str(island.value().attr("props")?).ok()?;
        let props = astro_decode(&raw);
        let manifest = props.get("manifest")?.as_str()?;
        let subtitles = props
            .get("subtitles")
            .and_then(Value::as_array)
            .map(|subs| {
                subs.iter()
                    .filter_map(|s| {
                        let src = fix_url(s.get("src")?.as_str()?, page);
                        Url::parse(&src).ok()?;
                        let name = s.get("name")?.as_str()?;
                        let lang = s.get("language")?.as_str()?;
                        Some(Track { url: src, lang: format!("{name} ({lang})") })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(PlayerData { manifest: fix_url(manifest, page), subtitles })
    })
}

// ================================ HLS =================================

/// The `KEY=VALUE` pairs of an HLS tag line. A quoted value may hold commas; the quotes are dropped.
type Attrs = Vec<(String, String)>;

fn attributes(line: &str) -> Attrs {
    let mut parts = Vec::new();
    let (mut start, mut quoted) = (0, false);
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            ',' if !quoted => {
                parts.push(&line[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&line[start..]);
    parts
        .into_iter()
        .filter_map(|part| {
            let (key, value) = part.split_once('=')?;
            let value = value.trim();
            let value = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')).unwrap_or(value);
            Some((key.trim().to_owned(), value.to_owned()))
        })
        .collect()
}

fn attr<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
}

/// A video variant of a master playlist, with the audio and subtitle tracks its group names.
#[derive(Debug, Clone, PartialEq)]
pub struct HlsVariant {
    pub url: String,
    pub attributes: Attrs,
    pub audio: Vec<Track>,
    pub subtitles: Vec<Track>,
}

const STREAM_INF: &str = "#EXT-X-STREAM-INF:";
const AUDIO_CODECS: [&str; 7] = ["mp4a", "opus", "vorbis", "ac-3", "ec-3", "flac", "alac"];

/// An absolute URI is kept byte for byte (signed URLs must not be re-encoded); a relative one is resolved.
fn resolve(uri: &str, base: &Url) -> Option<String> {
    let lower = uri.to_ascii_lowercase();
    if lower.starts_with("https://") || lower.starts_with("http://") {
        Some(uri.to_owned())
    } else {
        base.join(uri).ok().map(String::from)
    }
}

/// The tracks of one rendition type for a variant of `group`: the renditions of that group, plus the ones of groups
/// no variant references; a variant with no group, or a group with no rendition, gets every rendition of the type.
/// Audio is ordered DEFAULT first, then AUTOSELECT, keeping playlist order otherwise.
fn rendition_tracks(
    kind: &str,
    renditions: &[Attrs],
    variants: &[Attrs],
    base: &Url,
    group: Option<&str>,
) -> Vec<Track> {
    let mut of_type: Vec<&Attrs> = renditions.iter().filter(|r| attr(r, "TYPE") == Some(kind)).collect();
    if kind == "AUDIO" {
        of_type.sort_by_key(|r| (attr(r, "DEFAULT") != Some("YES"), attr(r, "AUTOSELECT") != Some("YES")));
    }
    let to_tracks = |list: Vec<&Attrs>| {
        let mut out: Vec<Track> = Vec::new();
        for r in list {
            let Some(url) = attr(r, "URI").and_then(|u| resolve(u, base)) else { continue };
            if out.iter().any(|t| t.url == url) {
                continue;
            }
            let lang = attr(r, "NAME").or(attr(r, "LANGUAGE")).or(attr(r, "GROUP-ID")).unwrap_or(kind);
            out.push(Track { url, lang: lang.to_owned() });
        }
        out
    };
    let own: Vec<&Attrs> = match group {
        Some(g) => of_type.iter().copied().filter(|r| attr(r, "GROUP-ID") == Some(g)).collect(),
        None => Vec::new(),
    };
    if own.is_empty() {
        return to_tracks(of_type);
    }
    let referenced: Vec<&str> = variants.iter().filter_map(|v| attr(v, kind)).collect();
    let unreferenced =
        of_type.iter().copied().filter(|r| attr(r, "GROUP-ID").is_none_or(|g| !referenced.contains(&g)));
    to_tracks(own.into_iter().chain(unreferenced).collect())
}

/// The video variants of a master playlist, or `None` for a media playlist. Audio-only variants are skipped.
pub fn parse_hls_master(playlist_url: &str, text: &str) -> Option<Vec<HlsVariant>> {
    let first = text.find(STREAM_INF)?;
    let base = Url::parse(playlist_url).ok()?;
    let streams: Vec<(&str, Attrs)> = text[first + STREAM_INF.len()..]
        .split(STREAM_INF)
        .map(|chunk| (chunk, attributes(chunk.lines().next().unwrap_or_default())))
        .collect();
    let renditions: Vec<Attrs> = text
        .lines()
        .map(str::trim)
        .filter_map(|l| l.strip_prefix("#EXT-X-MEDIA:"))
        .map(attributes)
        .collect();
    let variants: Vec<Attrs> = streams.iter().map(|(_, a)| a.clone()).collect();
    Some(
        streams
            .into_iter()
            .filter_map(|(chunk, attrs)| {
                if let Some(codecs) = attr(&attrs, "CODECS").filter(|c| !c.trim().is_empty())
                    && codecs.split(',').all(|c| AUDIO_CODECS.contains(&c.trim().split('.').next().unwrap_or_default()))
                {
                    return None;
                }
                let uri = chunk.lines().skip(1).map(str::trim).find(|l| !l.is_empty() && !l.starts_with('#'))?;
                Some(HlsVariant {
                    url: resolve(uri, &base)?,
                    audio: rendition_tracks("AUDIO", &renditions, &variants, &base, attr(&attrs, "AUDIO")),
                    subtitles: rendition_tracks("SUBTITLES", &renditions, &variants, &base, attr(&attrs, "SUBTITLES")),
                    attributes: attrs,
                })
            })
            .collect(),
    )
}

/// `standardQuality`: the closest standard height, as `720p`. A value that is not a number is kept.
pub fn standard_quality(height: &str) -> String {
    match height.trim().parse::<i64>() {
        Ok(h) => {
            let best =
                [144, 240, 360, 480, 720, 1080, 1440, 2160].into_iter().min_by_key(|q| (q - h).abs()).unwrap_or(h);
            format!("{best}p")
        }
        Err(_) => height.to_owned(),
    }
}

/// `formatBitrate`: bits per second as `3.10 Mbps`, `479 kbps` or `12 bps`; empty for zero or less.
pub fn format_bitrate(bps: i64) -> String {
    match bps {
        b if b >= 1_000_000 => format!("{:.2} Mbps", b as f64 / 1_000_000.0),
        b if b >= 1_000 => format!("{} kbps", b / 1_000),
        b if b > 0 => format!("{b} bps"),
        _ => String::new(),
    }
}

/// The digits after the first `x` of a `1920x1080` resolution, the height.
fn height_of(resolution: &str) -> Option<String> {
    resolution.char_indices().filter(|(_, c)| *c == 'x' || *c == 'X').find_map(|(i, _)| {
        let digits: String = resolution[i + 1..].chars().take_while(char::is_ascii_digit).collect();
        (!digits.is_empty()).then_some(digits)
    })
}

fn video(url: String, title: String, headers: &[(String, String)], subtitles: Vec<Track>, audio: Vec<Track>) -> Video {
    Video {
        video_url: url,
        video_title: title,
        resolution: None,
        bitrate: None,
        headers: headers.to_vec(),
        subtitle_tracks: subtitles,
        audio_tracks: audio,
    }
}

/// One video per quality of an HLS master playlist, highest bandwidth first, titled `{server} - 1080p (1920x1080) -
/// 3.10 Mbps`. A media playlist gives one video, `{server} - Video`.
pub fn hls_videos(
    server: &str,
    playlist_url: &str,
    text: &str,
    subtitles: &[Track],
    headers: &[(String, String)],
) -> Vec<Video> {
    let Some(variants) = parse_hls_master(playlist_url, text) else {
        return vec![video(playlist_url.to_owned(), format!("{server} - Video"), headers, subtitles.to_vec(), Vec::new())];
    };
    let mut out: Vec<(i64, Video)> = variants
        .into_iter()
        .map(|v| {
            let resolution = attr(&v.attributes, "RESOLUTION").map(|res| match height_of(res) {
                Some(h) => format!("{} ({res})", standard_quality(&h)),
                None => res.to_owned(),
            });
            let bandwidth = attr(&v.attributes, "AVERAGE-BANDWIDTH")
                .and_then(|b| b.parse::<i64>().ok())
                .filter(|b| *b > 0)
                .or_else(|| attr(&v.attributes, "BANDWIDTH").and_then(|b| b.parse().ok()));
            let rate = bandwidth.map(format_bitrate).filter(|r| !r.is_empty());
            let name = [resolution, rate].into_iter().flatten().collect::<Vec<_>>().join(" - ");
            let name = if name.trim().is_empty() { "Video".to_owned() } else { name };
            let height = attr(&v.attributes, "RESOLUTION").and_then(height_of).and_then(|h| h.parse().ok());
            let subs = subtitles.iter().cloned().chain(v.subtitles).collect();
            let mut item = video(v.url, format!("{server} - {name}"), headers, subs, v.audio);
            item.resolution = height;
            item.bitrate = bandwidth.and_then(|b| i32::try_from(b).ok());
            (bandwidth.unwrap_or(0), item)
        })
        .collect();
    out.sort_by_key(|(b, _)| Reverse(*b));
    out.into_iter().map(|(_, v)| v).collect()
}

// ================================ DASH ================================

/// One video per video representation of an MPD, titled `{server} - 1080p (1920x1080) - 3.10 Mbps`; the audio
/// representations become audio tracks. The address of each is its `BaseURL`, resolved against the MPD.
pub fn dash_videos(
    server: &str,
    mpd_url: &str,
    text: &str,
    subtitles: &[Track],
    headers: &[(String, String)],
) -> Vec<Video> {
    let Ok(rep_sel) = Selector::parse("representation") else {
        return Vec::new();
    };
    let base = Url::parse(mpd_url).ok();
    let doc = Html::parse_document(text);
    // The address is the representation's own BaseURL child; one without it (SegmentTemplate only) is skipped.
    let url_of = |el: scraper::ElementRef| {
        let raw = el
            .children()
            .filter_map(scraper::ElementRef::wrap)
            .find(|c| c.value().name() == "baseurl")?
            .text()
            .collect::<String>()
            .trim()
            .to_owned();
        Some(base.as_ref().and_then(|b| resolve(&raw, b)).unwrap_or(raw)).filter(|u| !u.is_empty())
    };
    // The mime type sits on the Representation or is inherited from its AdaptationSet.
    let mime = |el: scraper::ElementRef| {
        let own = el.value().attr("mimetype");
        let parent = el.parent().and_then(scraper::ElementRef::wrap).and_then(|p| p.value().attr("mimetype"));
        own.or(parent).unwrap_or_default().to_lowercase()
    };
    let bandwidth = |el: &scraper::ElementRef| el.value().attr("bandwidth").and_then(|b| b.parse::<i64>().ok());
    let audio: Vec<Track> = doc
        .select(&rep_sel)
        .filter(|el| mime(*el).contains("audio"))
        .filter_map(|el| Some(Track { lang: bandwidth(&el).map(format_bitrate).unwrap_or_default(), url: url_of(el)? }))
        .collect();
    doc.select(&rep_sel)
        .filter(|el| mime(*el).contains("video"))
        .filter_map(|el| {
            let url = url_of(el)?;
            let (w, h) = (el.value().attr("width").unwrap_or_default(), el.value().attr("height").unwrap_or_default());
            let res = format!("{} ({w}x{h})", standard_quality(h));
            let rate = bandwidth(&el).map(format_bitrate).filter(|r| !r.is_empty());
            let title = [Some(format!("{server} - {res}")), rate].into_iter().flatten().collect::<Vec<_>>().join(" - ");
            let mut item = video(url, title, headers, subtitles.to_vec(), audio.clone());
            item.resolution = h.parse().ok();
            item.bitrate = bandwidth(&el).and_then(|b| i32::try_from(b).ok());
            Some(item)
        })
        .collect()
}

// ================================ Sort ================================

/// The number before a `p` in a title (`1080p`), or 0.
fn title_height(title: &str) -> i64 {
    let bytes = title.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if bytes.get(i) == Some(&b'p') {
                return title[start..i].parse().unwrap_or(0);
            }
        } else {
            i += 1;
        }
    }
    0
}

/// The Kotlin `sortVideos` without its speed term (the titles here never carry a `KB/s` speed): the preferred server
/// first, then the preferred quality, then the height. The Kotlin sorts ascending and then reverses the list, so equal
/// keys come out in reverse order; this does the same.
pub fn sort_videos(mut videos: Vec<Video>, server: &str, quality: &str) -> Vec<Video> {
    let server = server.to_lowercase();
    videos.sort_by_key(|v| {
        let title = &v.video_title;
        (title.to_lowercase().contains(&server), title.contains(quality), title_height(title))
    });
    videos.reverse();
    videos
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(name: &str) -> &'static str {
        match name {
            "player_vid.html" => include_str!("../fixtures/player_vid.html"),
            "player_bird.html" => include_str!("../fixtures/player_bird.html"),
            "master.m3u8" => include_str!("../fixtures/master.m3u8"),
            other => panic!("no fixture {other}"),
        }
    }

    fn headers() -> Vec<(String, String)> {
        video_headers(&Url::parse("https://krussdomi.com/cat-player/player?id=1").unwrap())
    }

    #[test]
    fn the_current_player_gives_the_hls_manifest_and_subtitles() {
        let page = player_url(
            "https://krussdomi.com/cat-player/player?id=67d0c079169c31976b8d7970&source=vidstream&ln=ja-JP",
        )
        .unwrap();
        let data = parse_new_player(text("player_vid.html"), &page).unwrap();
        assert_eq!(data.manifest, "https://hls.krussdomi.com/manifest/67d0c079169c31976b8d7970/master.m3u8");
        assert_eq!(data.subtitles.len(), 9);
        assert_eq!(data.subtitles[0].lang, "English (en)");
        assert_eq!(
            data.subtitles[0].url,
            "https://subst.krussdomi.com/67d0c079169c31976b8d7970/67e0fa641967208e6c20882f.vtt"
        );
    }

    #[test]
    fn the_bird_player_gives_a_dash_manifest() {
        let page = Url::parse("https://krussdomi.com/cat-player/player?id=1").unwrap();
        let data = parse_new_player(text("player_bird.html"), &page).unwrap();
        assert_eq!(data.manifest, "https://bl.krussdomi.com/mpd/12975044/master.mpd");
        assert_eq!(data.subtitles.len(), 5);
    }

    #[test]
    fn a_page_without_an_island_is_not_the_current_player() {
        let page = Url::parse("https://p.test/x").unwrap();
        assert_eq!(parse_new_player("<html><body>cid: 'ab'</body></html>", &page), None);
    }

    #[test]
    fn the_master_playlist_becomes_one_video_per_quality_with_its_audio() {
        let master = "https://hls.krussdomi.com/manifest/67d0c079169c31976b8d7970/master.m3u8";
        let videos = hls_videos("VidStreaming", master, text("master.m3u8"), &[], &headers());
        assert!(videos[0].video_title.starts_with("VidStreaming - 1080p (1920x1080) - "), "{}", videos[0].video_title);
        assert_eq!(videos[0].resolution, Some(1080));
        assert!(videos[0].audio_tracks.len() > 1, "the audio renditions are listed");
        assert_eq!(videos[0].audio_tracks[0].lang, "Japanese", "the DEFAULT rendition comes first");
        let origin = videos[0].headers.iter().find(|(k, _)| k == "Origin").map(|(_, v)| v.as_str());
        assert_eq!(origin, Some("https://krussdomi.com"));
        assert!(videos[0].headers.iter().all(|(k, _)| k != "Referer"));
    }

    #[test]
    fn a_media_playlist_is_one_video() {
        let videos = hls_videos("S", "https://a.test/x.m3u8", "#EXTM3U\n#EXTINF:4,\nseg.ts\n", &[], &[]);
        assert_eq!(videos.len(), 1);
        assert_eq!((videos[0].video_url.as_str(), videos[0].video_title.as_str()), ("https://a.test/x.m3u8", "S - Video"));
    }

    #[test]
    fn a_quoted_attribute_may_hold_commas() {
        let attrs = attributes(r#"BANDWIDTH=5,CODECS="avc1.4d401f,mp4a.40.2",RESOLUTION=640x360"#);
        assert_eq!(attr(&attrs, "CODECS"), Some("avc1.4d401f,mp4a.40.2"));
        assert_eq!(attr(&attrs, "RESOLUTION"), Some("640x360"));
    }

    #[test]
    fn a_dash_manifest_becomes_one_video_per_representation() {
        let mpd = r#"<MPD><Period>
            <AdaptationSet><Representation mimeType="video/mp4" width="1920" height="1080" bandwidth="3000000"><BaseURL>v1080.mp4</BaseURL></Representation>
            <Representation mimeType="video/mp4" width="1280" height="720" bandwidth="1500000"><BaseURL>https://c.test/v720.mp4</BaseURL></Representation>
            <Representation mimeType="audio/mp4" bandwidth="128000"><BaseURL>a.mp4</BaseURL></Representation>
            </AdaptationSet></Period></MPD>"#;
        let videos = dash_videos("BirdStream", "https://bl.test/mpd/1/master.mpd", mpd, &[], &[]);
        assert_eq!(videos.len(), 2);
        assert_eq!(videos[0].video_title, "BirdStream - 1080p (1920x1080) - 3.00 Mbps");
        assert_eq!(videos[0].video_url, "https://bl.test/mpd/1/v1080.mp4");
        assert_eq!(videos[1].video_url, "https://c.test/v720.mp4");
        assert_eq!(videos[0].audio_tracks[0].url, "https://bl.test/mpd/1/a.mp4");
        assert_eq!(videos[0].audio_tracks[0].lang, "128 kbps");
    }

    #[test]
    fn dash_inherits_the_mime_type_and_skips_representations_without_a_base_url() {
        let mpd = r#"<MPD><Period><AdaptationSet mimeType="video/mp4">
            <Representation width="1280" height="720" bandwidth="1500000"><BaseURL>v720.mp4</BaseURL></Representation>
            <Representation width="640" height="360"><SegmentTemplate media="x"/></Representation>
            </AdaptationSet></Period></MPD>"#;
        let videos = dash_videos("B", "https://bl.test/m/master.mpd", mpd, &[], &[]);
        assert_eq!(videos.len(), 1);
        assert_eq!(videos[0].video_url, "https://bl.test/m/v720.mp4");
    }

    #[test]
    fn urls_from_players_are_normalised() {
        let base = Url::parse("https://krussdomi.com/cat-player/player").unwrap();
        assert_eq!(fix_url("https:////bl.test/a", &base), "https://bl.test/a");
        assert_eq!(fix_url("////bl.test/a", &base), "https://bl.test/a");
        assert_eq!(fix_url("//bl.test/a", &base), "https://bl.test/a");
        assert_eq!(fix_url("/subs/en.vtt", &base), "https://krussdomi.com/subs/en.vtt");
        assert_eq!(fix_url(" https://x.test/a ", &base), "https://x.test/a");
    }

    #[test]
    fn videos_sort_preferred_server_then_quality_then_height() {
        let mk = |url: &str, title: &str| video(url.into(), title.into(), &[], Vec::new(), Vec::new());
        let videos = vec![
            mk("1", "BirdStream - 1080p (1920x1080)"),
            mk("2", "VidStreaming - 360p (640x360)"),
            mk("3", "VidStreaming - 1080p (1920x1080)"),
            mk("4", "VidStreaming - 720p (1280x720)"),
        ];
        let urls: Vec<String> =
            sort_videos(videos, "VidStreaming", "1080p").into_iter().map(|v| v.video_url).collect();
        assert_eq!(urls, ["3", "4", "2", "1"]);
    }

    #[test]
    fn bitrates_and_standard_heights_are_formatted_like_kotlin() {
        assert_eq!(format_bitrate(3_100_000), "3.10 Mbps");
        assert_eq!(format_bitrate(479_000), "479 kbps");
        assert_eq!(format_bitrate(12), "12 bps");
        assert_eq!(format_bitrate(0), "");
        assert_eq!(standard_quality("1000"), "1080p");
        assert_eq!(standard_quality("ll"), "ll");
    }
}
