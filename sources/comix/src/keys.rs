// Ported from keiyoushi src/en/comix (Apache-2.0). Modified for the Kurobayomi plug-in contract; see README.md.
//! The request signature and the response cipher of the Comix API (site build `tmonyg`, checked 2026-10-10).
//!
//! - **Signature.** `_` is `"{prefix}." + base64url(HMAC-SHA256(key, path_and_query)[..12])`, no padding. The site never
//!   holds the key itself in the page, only the two SHA-256 states after the key block, so [`Keys`] keeps those and
//!   resumes the hash from them.
//! - **Cipher.** A body `{"e": base64}` decodes to two bytes `hi, lo` and the ciphertext. Byte `i` of the plain text is
//!   `ciphertext[i] ^ table[(hi * 256 + lo + i) % 1024]`.
//!
//! The values change when the site ships a new build. `capture.js` (plug-in root) reads them out of the live page.

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use serde::Deserialize;

const TABLE_LEN: usize = 1024;
/// The keys captured from build `tmonyg`.
const BUNDLED: &str = include_str!("keys/tmonyg.json");

/// The keys of one site build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keys {
    /// The site build these keys belong to (the `x-build-id` header).
    pub build: String,
    pub prefix: String,
    pub inner: [u32; 8],
    pub outer: [u32; 8],
    pub table: Vec<u8>,
}

#[derive(Deserialize)]
struct RawKeys {
    build: String,
    prefix: String,
    inner: [u32; 8],
    outer: [u32; 8],
    table: String,
}

impl Keys {
    /// The keys bundled with the plug-in.
    pub fn current() -> Result<Self, String> {
        Self::from_json(BUNDLED)
    }

    /// Reads the JSON that `capture.js` returns: `{"build","prefix","inner","outer","table"}`.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let raw: RawKeys = serde_json::from_str(text).map_err(|e| format!("comix keys: {e}"))?;
        let table = hex_decode(&raw.table)
            .filter(|t| t.len() == TABLE_LEN)
            .ok_or("comix keys: the table is not 1024 hex bytes")?;
        Ok(Self {
            build: raw.build,
            prefix: raw.prefix,
            inner: raw.inner,
            outer: raw.outer,
            table,
        })
    }

    /// The `_` parameter for an API path (with or without `/api/v1`) and its canonical query.
    pub fn sign(&self, path: &str, query: &str) -> String {
        let mut text = path.strip_prefix("/api/v1").unwrap_or(path).to_owned();
        if !query.is_empty() {
            text.push('?');
            text.push_str(query);
        }
        let inner = sha256_from(self.inner, 64, text.as_bytes());
        let mac = sha256_from(self.outer, 64, &inner);
        format!("{}.{}", self.prefix, URL_SAFE_NO_PAD.encode(&mac[..12]))
    }

    /// The plain bytes of an `e` value.
    pub fn decrypt(&self, value: &str) -> Result<Vec<u8>, String> {
        let data = STANDARD
            .decode(value.trim())
            .map_err(|e| format!("comix cipher text: {e}"))?;
        let [hi, lo, body @ ..] = data.as_slice() else {
            return Err("comix cipher text is shorter than its header".into());
        };
        let start = (usize::from(*hi) << 8 | usize::from(*lo)) % TABLE_LEN;
        Ok(body
            .iter()
            .enumerate()
            .map(|(i, b)| b ^ self.table[(start + i) % TABLE_LEN])
            .collect())
    }
}

fn hex_decode(text: &str) -> Option<Vec<u8>> {
    let text = text.trim();
    if text.len() % 2 != 0 {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// SHA-256 of `data`, started from `state` as if `already` bytes had been hashed before (a resumed hash). The `sha2`
/// crate does not take a starting state, so the compression function is written out here.
pub fn sha256_from(mut state: [u32; 8], already: u64, data: &[u8]) -> [u8; 32] {
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&((already + data.len() as u64) * 8).to_be_bytes());
    for block in message.chunks_exact(64) {
        compress(&mut state, block);
    }
    let mut out = [0u8; 32];
    for (chunk, s) in out.chunks_exact_mut(4).zip(state) {
        chunk.copy_from_slice(&s.to_be_bytes());
    }
    out
}

/// One SHA-256 compression of a 64-byte block.
fn compress(state: &mut [u32; 8], block: &[u8]) {
    let mut w = [0u32; 64];
    for (i, word) in block.chunks_exact(4).enumerate() {
        w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
    }
    for i in 16..64 {
        let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
        let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16]
            .wrapping_add(s0)
            .wrapping_add(w[i - 7])
            .wrapping_add(s1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for i in 0..64 {
        let t1 = h
            .wrapping_add(e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25))
            .wrapping_add((e & f) ^ (!e & g))
            .wrapping_add(K[i])
            .wrapping_add(w[i]);
        let t2 = (a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22))
            .wrapping_add((a & b) ^ (a & c) ^ (b & c));
        (h, g, f, e, d, c, b, a) = (g, f, e, d.wrapping_add(t1), c, b, a, t1.wrapping_add(t2));
    }
    for (s, v) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *s = s.wrapping_add(v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA256_IV: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn the_hash_from_the_standard_start_is_sha256() {
        assert_eq!(
            hex(&sha256_from(SHA256_IV, 0, b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(&sha256_from(SHA256_IV, 0, b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let long = [b'a'; 1000];
        assert_eq!(
            hex(&sha256_from(SHA256_IV, 0, &long)),
            "41edece42d63e8d9bf515a9ba6932e1c20cbc9f5a5d134645adb5db1b9737ea3"
        );
    }

    #[test]
    fn a_resumed_hash_is_hmac_when_started_from_the_key_states() {
        // HMAC-SHA256 with key "key" (RFC 4231 style check): the states after `key ^ ipad` and `key ^ opad`.
        let mut block = [0x36u8; 64];
        for (b, k) in block.iter_mut().zip(b"key") {
            *b ^= k;
        }
        let inner_state = state_after(&block);
        let mut block = [0x5cu8; 64];
        for (b, k) in block.iter_mut().zip(b"key") {
            *b ^= k;
        }
        let outer_state = state_after(&block);
        let inner = sha256_from(
            inner_state,
            64,
            b"The quick brown fox jumps over the lazy dog",
        );
        assert_eq!(
            hex(&sha256_from(outer_state, 64, &inner)),
            "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
        );
    }

    /// The SHA-256 state after one 64-byte block.
    fn state_after(block: &[u8; 64]) -> [u32; 8] {
        let mut state = SHA256_IV;
        compress(&mut state, block);
        state
    }

    /// Tokens the site's own `secure-tmonyg-*.js` produced in a browser on 2026-10-10 for these exact texts.
    #[test]
    fn the_signature_matches_what_the_site_made() {
        let keys = Keys::current().unwrap();
        let cases = [
            ("/manga", "limit=28&page=1", "17w.gZzX1AqVvudWzvBR"),
            ("/manga", "limit=28&page=2", "17w.3fmO16r25yLL7H7a"),
            ("/manga", "page=1", "17w.t28o8HEa6kUDxakB"),
            ("/manga", "", "17w.P1y7mxwCQQUxd00K"),
            ("/api/v1/chapters/123", "", "17w.eo1KiQZLbXzvVgYt"),
            (
                "/manga",
                "keyword= one piece &order[relevance]=desc&page=1",
                "17w.9RzX87R7fonqEirX",
            ),
            (
                "/manga",
                "keyword=naruto & co/é+&page=1",
                "17w.9Cq8M827sGNHQIjG",
            ),
            (
                "/manga",
                "limit=28&order[score]=desc&page=1&types[0]=manga&types[1]=manhwa",
                "17w.u5quudjb6F3zpM2l",
            ),
            (
                "/manga/px0j/chapters",
                "limit=100&order[number]=desc&page=1",
                "17w.Da_p75FFksj7QVyb",
            ),
        ];
        for (path, query, token) in cases {
            assert_eq!(keys.sign(path, query), token, "{path}?{query}");
        }
    }

    #[test]
    fn the_header_picks_the_start_in_the_table() {
        let keys = Keys::current().unwrap();
        // Ciphertext of zeros gives the table itself, from `(hi * 256 + lo) % 1024` on, wrapping at the end.
        let zeros = |hi: u8, lo: u8, n: usize| STANDARD.encode([vec![hi, lo], vec![0; n]].concat());
        assert_eq!(keys.decrypt(&zeros(0, 0, 1024)).unwrap(), keys.table);
        assert_eq!(keys.decrypt(&zeros(0, 33, 4)).unwrap(), keys.table[33..37]);
        assert_eq!(
            keys.decrypt(&zeros(255, 255, 2)).unwrap(),
            [keys.table[1023], keys.table[0]]
        );
        assert_eq!(
            keys.decrypt(&zeros(4, 1, 1)).unwrap(),
            [keys.table[1]],
            "1025 wraps to 1"
        );
        // The first table bytes the site's decoder gave for a forged body of zeros (header 0, 0).
        assert_eq!(hex(&keys.table[..8]), "d3151fa57a8e819a");
        assert!(keys.decrypt("AA").is_err(), "one byte has no header");
        assert!(keys.decrypt("not base64!").is_err());
    }

    #[test]
    fn a_real_encrypted_chapter_list_decrypts_to_the_plain_text_the_site_showed() {
        let value: serde_json::Value =
            serde_json::from_slice(include_bytes!("../fixtures/chapters_encrypted.json")).unwrap();
        let plain = Keys::current()
            .unwrap()
            .decrypt(value["e"].as_str().unwrap())
            .unwrap();
        assert_eq!(
            String::from_utf8(plain).unwrap(),
            r#"{"status":"ok","result":{"items":[],"meta":{"total":0,"perPage":100,"page":1,"lastPage":1,"from":0,"to":0,"hasNext":false,"hasPrev":false}}}"#
        );
    }

    #[test]
    fn captured_keys_round_trip_through_json() {
        let keys = Keys::current().unwrap();
        assert_eq!(
            (keys.build.as_str(), keys.prefix.as_str()),
            ("tmonyg", "17w")
        );
        let short = r#"{"build":"x","prefix":"1","inner":[0,0,0,0,0,0,0,0],"outer":[0,0,0,0,0,0,0,0],"table":"00ff"}"#;
        assert!(
            Keys::from_json(short).is_err(),
            "a table must be 1024 bytes"
        );
    }
}
