//! The video extractor as pure functions (`extractors/KickAssAnimeExtractor.kt` and the parts of `PlaylistUtils.kt`
//! it uses). Each server's `src` is a player page. Two kinds exist:
//!
//! - the current player (all servers seen on 2026-10-10): an Astro island whose `props` carry the manifest (HLS or
//!   DASH) and the subtitle tracks, read by [`parse_new_player`];
//! - the older player: a hex `cid` in the page gives the route of a `source.php` call signed with SHA-1
//!   ([`legacy_source_url`]), whose answer is AES-256-CBC encrypted JSON ([`decrypt_source`]).
//!
//! The manifest is then split per quality: [`hls_videos`] for a master playlist, [`dash_videos`] for an MPD.

use std::sync::LazyLock;

use aes::cipher::{BlockDecryptMut, KeyIvInit, block_padding::Pkcs7};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use kuro_source::SourceError;
use kuro_source::anime::{Headers, Track, Video};
use regex::Regex;
use scraper::{Html, Selector};
use serde::Deserialize;
use serde_json::Value;
use sha1::{Digest, Sha1};
use url::Url;

/// The user agent the Kotlin extractor sends with every stream request.
pub const VIDEO_USER_AGENT: &str =
    "Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/129.0.0.0 Mobile Safari/537.36";

/// The servers the site names (Kotlin `SERVERS`); the first is the preferred one.
pub const SERVERS: [&str; 4] = ["VidStreaming", "CatStream", "BirdStream", "DuckStream"];

/// The player page of a server `src`: an ad (`/vast`) address is turned into the player itself.
pub fn player_url(src: &str) -> Result<Url, SourceError> {
    let mut url = Url::parse(src).map_err(|e| SourceError::Url(e.to_string()))?;
    if src.contains("/vast") {
        url.set_path("/cat-player/player");
    }
    Ok(url)
}

/// The headers the player must send with the playlist, the segments and the subtitles (Kotlin `getVideoHeaders`):
/// no Referer, the Origin of the player page and a fixed mobile Chrome user agent.
pub fn video_headers(player: &Url) -> Vec<(String, String)> {
    let host = player.host_str().unwrap_or_default();
    [
        ("Accept", "*/*".to_owned()),
        ("Accept-Language", "en-US,en;q=0.9".to_owned()),
        ("Origin", format!("https://{host}")),
        ("Sec-Fetch-Dest", "empty".to_owned()),
        ("Sec-Fetch-Mode", "cors".to_owned()),
        ("Sec-Fetch-Site", "same-site".to_owned()),
        ("User-Agent", VIDEO_USER_AGENT.to_owned()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v))
    .collect()
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

// ============================ Current player ===========================

/// What the current player page gives: the manifest address and the subtitle tracks.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerData {
    pub manifest: String,
    pub subtitles: Vec<Track>,
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

static ISLAND: LazyLock<Selector> = LazyLock::new(|| Selector::parse("astro-island[props]").expect("selector"));

/// The manifest and subtitles of the current player (Kotlin `parseNewPlayer`), or `None` when the page has no island
/// with a manifest (then the page is the older player).
pub fn parse_new_player(html: &str, page: &Url) -> Option<PlayerData> {
    let doc = Html::parse_document(html);
    doc.select(&ISLAND).find_map(|island| {
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

// ============================= Older player ============================

/// The AES key of a server of the older player. CatStream has none, so it has no videos there.
pub fn legacy_key(server: &str) -> Option<&'static [u8]> {
    match server {
        "VidStreaming" => Some(b"e13d38099bf562e8b9851a652d2043d3"),
        "DuckStream" => Some(b"4504447b74641ad972980a6b8ffd7631"),
        "BirdStream" => Some(b"4b14d0ff625163e3c9c7a47926484bf2"),
        _ => None,
    }
}

fn hex_decode(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok()).collect()
}

fn between<'a>(text: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let from = text.find(start)? + start.len();
    let len = text[from..].find(end)?;
    Some(&text[from..from + len])
}

/// The signed `source.php` address of the older player (Kotlin `getSignature` and the URL around it). `cid` in the
/// page is hex of `ip|route`; the SHA-1 runs over ip, user agent, route, id, timestamp (not for BirdStream) and key.
/// `now_secs` is the Unix time; the timestamp sent is one minute ahead.
pub fn legacy_source_url(html: &str, player: &Url, server: &str, user_agent: &str, now_secs: u64) -> Option<String> {
    let key = legacy_key(server)?;
    let mid = if server == "DuckStream" { "mid" } else { "id" };
    let bird = server == "BirdStream";
    let query = player.query_pairs().find(|(k, _)| k == mid)?.1.into_owned();
    let cid = String::from_utf8(hex_decode(between(html, "cid: '", "'")?)?).ok()?;
    let mut parts = cid.split('|');
    let (ip, route) = (parts.next()?, parts.next()?.replace("player.php", "source.php"));
    let timestamp = (now_secs + 60).to_string();
    let key = std::str::from_utf8(key).ok()?;
    let signed = if bird {
        format!("{ip}{user_agent}{route}{query}{key}")
    } else {
        format!("{ip}{user_agent}{route}{query}{timestamp}{key}")
    };
    let digest = Sha1::digest(signed.as_bytes());
    let sig: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    let host = player.host_str()?;
    let mut url = format!("https://{host}{route}?{mid}={query}");
    if !bird {
        url.push_str(&format!("&e={timestamp}"));
    }
    url.push_str(&format!("&s={sig}"));
    Some(url)
}

/// The decrypted answer of `source.php` (Kotlin `VideoDto`).
#[derive(Deserialize, Debug, Default, PartialEq)]
pub struct SourceDto {
    #[serde(default)]
    pub hls: String,
    #[serde(default)]
    pub dash: String,
    #[serde(default)]
    pub subtitles: Vec<SubtitleDto>,
}

#[derive(Deserialize, Debug, PartialEq)]
pub struct SubtitleDto {
    pub name: String,
    pub language: String,
    pub src: String,
}

/// Decrypts a `source.php` answer: `{"data":"<base64>:<hex iv>"}`, AES-CBC with PKCS#7 padding and the server's key
/// (Kotlin `CryptoAES.decrypt(data, key, iv)`).
pub fn decrypt_source(body: &str, key: &[u8]) -> Result<SourceDto, SourceError> {
    let bad = |what: &str| SourceError::Parse(format!("source answer: {what}"));
    let packed = body.split_once(":\"").map(|(_, rest)| rest).ok_or_else(|| bad("no data"))?;
    let packed = packed.split('"').next().unwrap_or_default().replace('\\', "");
    let (data, iv) = packed.split_once(':').ok_or_else(|| bad("no iv"))?;
    let iv = hex_decode(iv).ok_or_else(|| bad("iv is not hex"))?;
    let mut buf = STANDARD.decode(data).map_err(|_| bad("data is not base64"))?;
    let plain = match key.len() {
        16 => cbc::Decryptor::<aes::Aes128>::new_from_slices(key, &iv)
            .map_err(|_| bad("key or iv length"))?
            .decrypt_padded_mut::<Pkcs7>(&mut buf),
        24 => cbc::Decryptor::<aes::Aes192>::new_from_slices(key, &iv)
            .map_err(|_| bad("key or iv length"))?
            .decrypt_padded_mut::<Pkcs7>(&mut buf),
        _ => cbc::Decryptor::<aes::Aes256>::new_from_slices(key, &iv)
            .map_err(|_| bad("key or iv length"))?
            .decrypt_padded_mut::<Pkcs7>(&mut buf),
    }
    .map_err(|_| bad("decryption failed"))?;
    serde_json::from_slice(plain).map_err(|e| SourceError::Parse(e.to_string()))
}

/// The subtitle tracks of a decrypted answer, labelled `Name (lang)`.
pub fn legacy_subtitles(dto: &SourceDto, player: &Url) -> Vec<Track> {
    dto.subtitles
        .iter()
        .map(|s| Track { url: fix_url(&s.src, player), lang: format!("{} ({})", s.name, s.language) })
        .collect()
}

// ================================ HLS =================================

/// A video variant of a master playlist (`HlsVariant.kt`).
#[derive(Debug, Clone, PartialEq)]
pub struct HlsVariant {
    pub url: String,
    pub attributes: Vec<(String, String)>,
    pub audio: Vec<Track>,
    pub subtitles: Vec<Track>,
}

impl HlsVariant {
    fn attr(&self, name: &str) -> Option<&str> {
        attr(&self.attributes, name)
    }
}

fn attr<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
}

static HLS_ATTRIBUTE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"([A-Z0-9-]+)=(?:"([^"]*)"|([^,\r\n]*))"#).expect("regex"));
const STREAM_INF: &str = "#EXT-X-STREAM-INF:";
const AUDIO_CODECS: [&str; 7] = ["mp4a", "opus", "vorbis", "ac-3", "ec-3", "flac", "alac"];

fn hls_attributes(line: &str) -> Vec<(String, String)> {
    HLS_ATTRIBUTE
        .captures_iter(line)
        .map(|c| {
            let value = c.get(2).map(|m| m.as_str().to_owned()).unwrap_or_else(|| c[3].trim().to_owned());
            (c[1].to_owned(), value)
        })
        .collect()
}

/// An absolute URI is kept byte for byte (signed URLs must not be re-encoded); a relative one is resolved.
fn resolve(uri: &str, base: &Url) -> Option<String> {
    let lower = uri.to_ascii_lowercase();
    if lower.starts_with("https://") || lower.starts_with("http://") {
        Some(uri.to_owned())
    } else {
        base.join(uri).ok().map(String::from)
    }
}

/// The tracks of one rendition type, per referenced group: the group's own renditions plus the ones of groups no
/// variant references; a variant without a known group gets every rendition of the type. Audio is ordered DEFAULT
/// first, then AUTOSELECT, keeping playlist order otherwise.
fn rendition_tracks(
    kind: &str,
    renditions: &[Vec<(String, String)>],
    variants: &[Vec<(String, String)>],
    base: &Url,
) -> impl Fn(Option<&str>) -> Vec<Track> {
    let mut of_type: Vec<&Vec<(String, String)>> =
        renditions.iter().filter(|r| attr(r, "TYPE") == Some(kind)).collect();
    if kind == "AUDIO" {
        of_type.sort_by_key(|r| (attr(r, "DEFAULT") != Some("YES"), attr(r, "AUTOSELECT") != Some("YES")));
    }
    let referenced: Vec<&str> = variants.iter().filter_map(|v| attr(v, kind)).collect();
    let to_tracks = |list: Vec<&Vec<(String, String)>>| {
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
    let all = to_tracks(of_type.clone());
    let unreferenced: Vec<_> =
        of_type.iter().copied().filter(|r| attr(r, "GROUP-ID").is_none_or(|g| !referenced.contains(&g))).collect();
    let mut by_group: Vec<(String, Vec<Track>)> = Vec::new();
    for r in &of_type {
        let Some(group) = attr(r, "GROUP-ID") else { continue };
        if !referenced.contains(&group) || by_group.iter().any(|(g, _)| g == group) {
            continue;
        }
        let mut list: Vec<_> = of_type.iter().copied().filter(|x| attr(x, "GROUP-ID") == Some(group)).collect();
        list.extend(unreferenced.iter().copied());
        by_group.push((group.to_owned(), to_tracks(list)));
    }
    move |group| {
        group
            .and_then(|g| by_group.iter().find(|(name, _)| name == g))
            .map(|(_, t)| t.clone())
            .unwrap_or_else(|| all.clone())
    }
}

/// The video variants of a master playlist (`parseHlsMasterPlaylist`), or `None` for a media playlist. Audio-only
/// variants are skipped.
pub fn parse_hls_master(playlist_url: &str, text: &str) -> Option<Vec<HlsVariant>> {
    let first = text.find(STREAM_INF)?;
    let base = Url::parse(playlist_url).ok()?;
    let streams: Vec<(&str, Vec<(String, String)>)> = text[first + STREAM_INF.len()..]
        .split(STREAM_INF)
        .map(|chunk| (chunk, hls_attributes(chunk.lines().next().unwrap_or_default())))
        .collect();
    let renditions: Vec<Vec<(String, String)>> =
        text.lines().map(str::trim).filter_map(|l| l.strip_prefix("#EXT-X-MEDIA:")).map(hls_attributes).collect();
    let variant_attrs: Vec<Vec<(String, String)>> = streams.iter().map(|(_, a)| a.clone()).collect();
    let audio = rendition_tracks("AUDIO", &renditions, &variant_attrs, &base);
    let subtitles = rendition_tracks("SUBTITLES", &renditions, &variant_attrs, &base);
    Some(
        streams
            .into_iter()
            .filter_map(|(chunk, attributes)| {
                if let Some(codecs) = attr(&attributes, "CODECS").filter(|c| !c.trim().is_empty())
                    && codecs.split(',').all(|c| AUDIO_CODECS.contains(&c.trim().split('.').next().unwrap_or_default()))
                {
                    return None;
                }
                let uri = chunk.lines().skip(1).map(str::trim).find(|l| !l.is_empty() && !l.starts_with('#'))?;
                Some(HlsVariant {
                    url: resolve(uri, &base)?,
                    audio: audio(attr(&attributes, "AUDIO")),
                    subtitles: subtitles(attr(&attributes, "SUBTITLES")),
                    attributes,
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

fn video(url: String, title: String, headers: &[(String, String)], subtitles: Vec<Track>, audio: Vec<Track>) -> Video {
    Video {
        headers: Some(Headers(headers.to_vec())),
        subtitle_tracks: subtitles,
        audio_tracks: audio,
        initialized: true,
        ..Video::new(url, title)
    }
}

/// One video per quality of an HLS master playlist (`extractFromHls`), highest bandwidth first, titled
/// `{server} - 1080p (1920x1080) - 3.10 Mbps`. A media playlist gives one video, `{server} - Video`.
pub fn hls_videos(
    server: &str,
    playlist_url: &str,
    text: &str,
    subtitles: &[Track],
    headers: &[(String, String)],
) -> Vec<Video> {
    let Some(variants) = parse_hls_master(playlist_url, text) else {
        return vec![video(
            playlist_url.to_owned(),
            format!("{server} - Video"),
            headers,
            subtitles.to_vec(),
            Vec::new(),
        )];
    };
    static HEIGHT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[xX](\d+)").expect("regex"));
    let mut out: Vec<(i64, Video)> = variants
        .into_iter()
        .map(|v| {
            let resolution = v.attr("RESOLUTION").map(|res| match HEIGHT.captures(res) {
                Some(c) => format!("{} ({res})", standard_quality(&c[1])),
                None => res.to_owned(),
            });
            let bandwidth = v
                .attr("AVERAGE-BANDWIDTH")
                .and_then(|b| b.parse::<i64>().ok())
                .filter(|b| *b > 0)
                .or_else(|| v.attr("BANDWIDTH").and_then(|b| b.parse().ok()));
            let rate = bandwidth.map(format_bitrate).filter(|r| !r.is_empty());
            let name = [resolution, rate].into_iter().flatten().collect::<Vec<_>>().join(" - ");
            let name = if name.trim().is_empty() { "Video".to_owned() } else { name };
            let height = v.attr("RESOLUTION").and_then(|r| HEIGHT.captures(r)).and_then(|c| c[1].parse().ok());
            let subs = subtitles.iter().cloned().chain(v.subtitles).collect();
            let mut item = video(v.url, format!("{server} - {name}"), headers, subs, v.audio);
            item.resolution = height;
            item.bitrate = bandwidth.and_then(|b| i32::try_from(b).ok());
            (bandwidth.unwrap_or(0), item)
        })
        .collect();
    out.sort_by_key(|(b, _)| std::cmp::Reverse(*b));
    out.into_iter().map(|(_, v)| v).collect()
}

// ================================ DASH ================================

/// One video per video representation of an MPD (`extractFromDash`), titled `{server} - 1080p (1920x1080) - 3.10
/// Mbps`; the audio representations become audio tracks. The address of each is its `BaseURL`, resolved against the
/// MPD.
pub fn dash_videos(
    server: &str,
    mpd_url: &str,
    text: &str,
    subtitles: &[Track],
    headers: &[(String, String)],
) -> Vec<Video> {
    static AUDIO: LazyLock<Selector> =
        LazyLock::new(|| Selector::parse(r#"representation[mimetype*="audio"]"#).expect("sel"));
    static VIDEO: LazyLock<Selector> =
        LazyLock::new(|| Selector::parse(r#"representation[mimetype*="video"]"#).expect("sel"));
    let base = Url::parse(mpd_url).ok();
    let doc = Html::parse_document(text);
    let url_of = |el: scraper::ElementRef| {
        let raw = el.text().collect::<String>().trim().to_owned();
        base.as_ref().and_then(|b| resolve(&raw, b)).unwrap_or(raw)
    };
    let bandwidth = |el: &scraper::ElementRef| el.value().attr("bandwidth").and_then(|b| b.parse::<i64>().ok());
    let audio: Vec<Track> = doc
        .select(&AUDIO)
        .map(|el| Track { lang: bandwidth(&el).map(format_bitrate).unwrap_or_default(), url: url_of(el) })
        .collect();
    doc.select(&VIDEO)
        .map(|el| {
            let (w, h) = (el.value().attr("width").unwrap_or_default(), el.value().attr("height").unwrap_or_default());
            let res = format!("{} ({w}x{h})", standard_quality(h));
            let rate = bandwidth(&el).map(format_bitrate).filter(|r| !r.is_empty());
            let title = [Some(format!("{server} - {res}")), rate].into_iter().flatten().collect::<Vec<_>>().join(" - ");
            let mut item = video(url_of(el), title, headers, subtitles.to_vec(), audio.clone());
            item.resolution = h.parse().ok();
            item.bitrate = bandwidth(&el).and_then(|b| i32::try_from(b).ok());
            item
        })
        .collect()
}

// ================================ Sort ================================

/// The Kotlin `sortVideos`: the preferred server first, then the preferred quality, then the height, then the speed.
pub fn sort_videos(mut videos: Vec<Video>, server: &str, quality: &str) -> Vec<Video> {
    static RES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d+)p").expect("regex"));
    static SPEED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"([\d,]+) [KMGTPE]B/s").expect("regex"));
    let server = server.to_lowercase();
    let key = |v: &Video| {
        let title = &v.video_title;
        let res: i64 = RES.captures(title).and_then(|c| c[1].parse().ok()).unwrap_or(0);
        let speed: f64 = SPEED.captures(title).and_then(|c| c[1].replace(',', ".").parse().ok()).unwrap_or(0.0);
        (title.to_lowercase().contains(&server), title.contains(quality), res, speed)
    };
    videos.sort_by(|a, b| key(b).partial_cmp(&key(a)).unwrap_or(std::cmp::Ordering::Equal));
    videos
}
