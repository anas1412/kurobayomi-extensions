//! Kwik decode, ported from KwikExtractor.kt (yuzono_anime-extensions, Apache-2.0), which is based on CloudStream's
//! AnimePaheProvider (MIT). Only the string work: the fetches, the Cloudflare retry and the POST stay with the host.
//!
//! The MIT notice for the CloudStream part:
//!
//! MIT License
//!
//! Copyright (c) 2021 Osten
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy
//! of this software and associated documentation files (the "Software"), to deal
//! in the Software without restriction, including without limitation the rights
//! to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
//! copies of the Software, and to permit persons to whom the Software is
//! furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in all
//! copies or substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
//! IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
//! FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
//! AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
//! LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
//! SOFTWARE.

use regex_lite::Regex;
use std::collections::HashMap;

#[allow(dead_code)] // the pahe.win MP4 path: kept for downloads, not used for streaming (HLS)
/// The first `("payload",n,"key",v1,v2,n)` call of a Kwik page. `v2` is both the radix and the index of the separator in `key`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Params {
    pub payload: String,
    pub key: String,
    pub v1: i32,
    pub v2: i32,
}

#[allow(dead_code)] // the pahe.win MP4 path: kept for downloads, not used for streaming (HLS)
pub fn find_params(html: &str) -> Option<Params> {
    let re = Regex::new(r#"\("([A-Za-z0-9_]+)",[0-9]+,"([A-Za-z0-9_]+)",([0-9]+),([0-9]+),[0-9]+\)"#).unwrap();
    let c = re.captures(html)?;
    Some(Params {
        payload: c[1].to_string(),
        key: c[2].to_string(),
        v1: c[3].parse().unwrap_or(0),
        v2: c[4].parse().unwrap_or(0),
    })
}

#[allow(dead_code)] // the pahe.win MP4 path: kept for downloads, not used for streaming (HLS)
/// The key routine. `payload` is cut at each character `key[v2]`; each piece is the key indices of one character, written
/// in radix `v2`, minus `v1`. Stops at the first piece that does not parse, as the reference does.
/// None when `key[v2]` does not exist (the reference throws there).
pub fn decrypt(payload: &str, key: &str, v1: i32, v2: i32) -> Option<String> {
    let key: Vec<char> = key.chars().collect();
    let sep = *key.get(usize::try_from(v2).ok()?)?;
    if !(2..=36).contains(&v2) {
        return Some(String::new()); // the reference's toInt throws on the first piece and breaks out
    }
    let index: HashMap<char, usize> = key.iter().enumerate().map(|(i, &c)| (c, i)).collect();
    let chars: Vec<char> = payload.chars().collect();
    let mut out: Vec<u16> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let Some(rel) = chars[i..].iter().position(|&c| c == sep) else { break };
        let end = i + rel;
        let mut digits = String::new();
        for c in &chars[i..end] {
            match index.get(c) {
                Some(d) => digits.push_str(&d.to_string()),
                None => digits.push_str("-1"),
            }
        }
        i = end + 1;
        let Ok(code) = i32::from_str_radix(&digits, v2 as u32) else { break };
        out.push(code.wrapping_sub(v1) as u16); // Kotlin's Int.toChar keeps the low 16 bits
    }
    Some(String::from_utf16_lossy(&out))
}

#[allow(dead_code)] // the pahe.win MP4 path: kept for downloads, not used for streaming (HLS)
/// The `action` URL and the `_token` value of the decrypted Kwik form. None if either is missing.
pub fn form_fields(decrypted: &str) -> Option<(String, String)> {
    let action = Regex::new(r#"action="([^"]+)""#).unwrap();
    let token = Regex::new(r#"value="([^"]+)""#).unwrap();
    Some((action.captures(decrypted)?[1].to_string(), token.captures(decrypted)?[1].to_string()))
}

#[allow(dead_code)] // the pahe.win MP4 path: kept for downloads, not used for streaming (HLS)
/// The Kwik URL from the `location` header of a pahe `/i` redirect: `https://` plus the text after the last `https://`.
pub fn kwik_url_from_location(location: &str) -> String {
    let tail = location.rsplit_once("https://").map_or(location, |(_, t)| t);
    format!("https://{tail}")
}

/// The stream URL in an unpacked Kwik script (`const source=\'URL\';`). None when the marker is missing.
/// (The reference returns the whole text in that case.)
pub fn hls_source(unpacked: &str) -> Option<&str> {
    let (_, rest) = unpacked.split_once(r"const source=\'")?;
    rest.split_once(r"\';").map(|(url, _)| url)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "kX3qZ8bR0yL1mW7aN5tJ2cHsVdUfGpE9";
    const PLAIN: &str = r#"<form action="https://kwik.cx/d/Ab12" method="POST"><input name="_token" value="tok_9Zz"></form>"#;

    /// Inverse of `decrypt` for radix 8 (v2 = 8): each code, written in octal, one key character per digit, then the separator.
    fn encode(plain: &str, key: &str, v1: i32, v2: i32) -> String {
        let key: Vec<char> = key.chars().collect();
        let mut out = String::new();
        for ch in plain.chars() {
            for d in format!("{:o}", ch as i32 + v1).chars() {
                out.push(key[d.to_digit(8).unwrap() as usize]);
            }
            out.push(key[v2 as usize]);
        }
        out
    }

    #[test]
    fn decrypt_inverts_the_encoding() {
        let payload = encode(PLAIN, KEY, 5, 8);
        assert_eq!(decrypt(&payload, KEY, 5, 8).as_deref(), Some(PLAIN));
    }

    #[test]
    fn page_to_form_chain() {
        let payload = encode(PLAIN, KEY, 5, 8);
        let html = format!(r#"<script>var x=("{payload}",62,"{key}",5,8,0);</script>"#, key = KEY);
        let p = find_params(&html).unwrap();
        assert_eq!((p.v1, p.v2), (5, 8));
        let decrypted = decrypt(&p.payload, &p.key, p.v1, p.v2).unwrap();
        assert_eq!(
            form_fields(&decrypted),
            Some(("https://kwik.cx/d/Ab12".to_string(), "tok_9Zz".to_string())),
        );
    }

    #[test]
    fn missing_key_index_and_missing_fields_are_none() {
        assert_eq!(decrypt("ab", "xyz", 0, 8), None);
        assert_eq!(form_fields("<form method=\"POST\"></form>"), None);
        assert!(find_params("no call here").is_none());
    }

    #[test]
    fn redirect_location_becomes_the_kwik_url() {
        assert_eq!(kwik_url_from_location("https://animepahe.pw/go?https://kwik.cx/f/abc"), "https://kwik.cx/f/abc");
        assert_eq!(kwik_url_from_location("kwik.cx/f/x"), "https://kwik.cx/f/x");
    }

    #[test]
    fn hls_source_is_between_the_markers() {
        let unpacked = r#"var a=1;const source=\'https://vod.example/a/index.m3u8\';var b=2;"#;
        assert_eq!(hls_source(unpacked), Some("https://vod.example/a/index.m3u8"));
        assert_eq!(hls_source("nothing here"), None);
    }
}
