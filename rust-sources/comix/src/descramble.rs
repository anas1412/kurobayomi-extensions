//! Page images that come back XOR-encoded or cut into a shuffled 5x5 grid (`Descrambler.kt`). The image host says so
//! in response headers; [`unscramble`] reads them and returns the image to show. All Kotlin `Int` arithmetic is 32-bit
//! wrapping, kept here with `wrapping_*` and `u32` shifts for `ushr`.

use http::HeaderMap;
use image::ImageEncoder;

const GRID: u32 = 5;
const TILES: usize = (GRID * GRID) as usize;

fn header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

/// Kotlin `toLongOrNull()?.toInt()`: parse as 64-bit, keep the low 32 bits.
fn int_header(headers: &HeaderMap, name: &str) -> Option<i32> {
    header(headers, name)?.parse::<i64>().ok().map(|v| v as i32)
}

/// The image bytes to show for a page response with these headers. `Ok(None)`: the body needs nothing. `Err`: a grid
/// image that cannot be decoded (Kotlin answers a 500 "Failed to decode image").
pub fn unscramble(body: &[u8], headers: &HeaderMap) -> Result<Option<Vec<u8>>, String> {
    let enc_seed = int_header(headers, "x-enc-seed");
    let enc_len = header(headers, "x-enc-len").and_then(|v| v.parse::<i32>().ok());
    let grid_seed = int_header(headers, "x-scramble-seed");
    let grid_algo = header(headers, "x-scramble-algo");
    let needs_xor = matches!(enc_seed, Some(s) if s != 0) && enc_len.is_some();
    let grid = header(headers, "x-scramble-grid") == Some("5x5")
        && matches!(grid_algo, None | Some("1" | "2" | "3"))
        && matches!(grid_seed, Some(s) if s != 0);
    if !needs_xor && !grid {
        return Ok(None);
    }
    let bytes = match (enc_seed, enc_len) {
        (Some(seed), Some(len)) if needs_xor => {
            decode_encoded_bytes(body, seed, len, header(headers, "x-enc-algo"))
        }
        _ => body.to_vec(),
    };
    if !grid {
        // ponytail: Kotlin re-encodes the decoded image as JPEG here; the reader decodes any format, so the bytes go as they are.
        return Ok(Some(bytes));
    }
    let seed = grid_seed.unwrap_or(0) ^ scramble_hash(header(headers, "x-scramble-hash"));
    let image = image::load_from_memory(&bytes).map_err(|_| "Failed to decode image".to_owned())?;
    let fixed = descramble(&image.to_rgb8(), seed, grid_algo);
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90)
        .write_image(
            fixed.as_raw(),
            fixed.width(),
            fixed.height(),
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|e| e.to_string())?;
    Ok(Some(out))
}

/// The XOR layer. Algo "2" tries three xorshift variants and the LCG and keeps the first that starts like an image.
pub fn decode_encoded_bytes(bytes: &[u8], seed: i32, length: i32, algo: Option<&str>) -> Vec<u8> {
    if algo != Some("2") {
        return decode_lcg(bytes, seed, length);
    }
    let candidates = [
        decode_xorshift(bytes, seed | 1, length, false),
        decode_xorshift(bytes, seed, length, false),
        decode_xorshift(bytes, seed | 1, length, true),
        decode_lcg(bytes, seed, length),
    ];
    let pick = candidates
        .iter()
        .position(|c| has_image_signature(c))
        .unwrap_or(0);
    candidates.into_iter().nth(pick).unwrap_or_default()
}

fn limit(bytes: &[u8], length: i32) -> usize {
    bytes.len().min(usize::try_from(length).unwrap_or(0))
}

fn decode_xorshift(bytes: &[u8], initial: i32, length: i32, high_byte: bool) -> Vec<u8> {
    let mut out = bytes.to_vec();
    let mut state = initial as u32;
    for b in out.iter_mut().take(limit(bytes, length)) {
        state = xorshift(state);
        *b ^= if high_byte {
            (state >> 24) as u8
        } else {
            state as u8
        };
    }
    out
}

fn decode_lcg(bytes: &[u8], seed: i32, length: i32) -> Vec<u8> {
    let mut out = bytes.to_vec();
    let mut state = seed as u32;
    for b in out.iter_mut().take(limit(bytes, length)) {
        state = state.wrapping_mul(1_000_005).wrapping_add(1_234_567_891);
        *b ^= (state >> 24) as u8;
    }
    out
}

fn xorshift(mut state: u32) -> u32 {
    state ^= state << 13;
    state ^= state >> 17;
    state ^ (state << 5)
}

fn scramble_hash(hash: Option<&str>) -> i32 {
    match hash.map(str::trim) {
        Some("03632") => 58414,
        Some("02900") => 117_532,
        _ => 0,
    }
}

fn has_image_signature(b: &[u8]) -> bool {
    b.len() >= 12
        && (b.starts_with(b"RIFF") && &b[8..12] == b"WEBP"
            || b.starts_with(&[0xFF, 0xD8])
            || b.starts_with(b"\x89PNG"))
}

/// For each destination tile, the source tile it is copied from. Algo "3" shuffles with xorshift, the others with an LCG.
pub fn tile_order(seed: i32, algo: Option<&str>) -> [usize; TILES] {
    let mut arr: [usize; TILES] = std::array::from_fn(|i| i);
    let mut state = if algo == Some("3") {
        (seed | 1) as u32
    } else {
        seed as u32
    };
    for i in (1..TILES).rev() {
        state = if algo == Some("3") {
            xorshift(state)
        } else {
            state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223)
        };
        arr.swap(i, state as usize % (i + 1));
    }
    let mut inverse = [0; TILES];
    for (i, &v) in arr.iter().enumerate() {
        inverse[v] = i;
    }
    inverse
}

/// Puts the tiles back. Pixels right and below the last whole tile stay where they are, as on the Kotlin canvas.
pub fn descramble(image: &image::RgbImage, seed: i32, algo: Option<&str>) -> image::RgbImage {
    let (tile_w, tile_h) = (image.width() / GRID, image.height() / GRID);
    let mut out = image.clone();
    for (dst, &src) in tile_order(seed, algo).iter().enumerate() {
        let (sx, sy) = ((src as u32 % GRID) * tile_w, (src as u32 / GRID) * tile_h);
        let (dx, dy) = ((dst as u32 % GRID) * tile_w, (dst as u32 / GRID) * tile_h);
        for y in 0..tile_h {
            for x in 0..tile_w {
                out.put_pixel(dx + x, dy + y, *image.get_pixel(sx + x, sy + y));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Expected values from a separate 32-bit integer model of `Descrambler.kt` (Python, masking every step to 32 bits).
    #[test]
    fn the_tile_orders_match_the_kotlin_model() {
        assert_eq!(tile_order(12345, None), ORDER_LCG_12345);
        assert_eq!(tile_order(12345, Some("3")), ORDER_XS_12345);
        assert_eq!(tile_order(-7, Some("1")), ORDER_LCG_NEG7);
    }

    #[test]
    fn the_xor_streams_match_the_kotlin_model() {
        let zeros = [0u8; 8];
        assert_eq!(decode_lcg(&zeros, 987_654_321, 8), LCG_987654321);
        assert_eq!(
            decode_xorshift(&zeros, 987_654_321, 8, false),
            XS_LOW_987654321
        );
        assert_eq!(
            decode_xorshift(&zeros, 987_654_321, 8, true),
            XS_HIGH_987654321
        );
        assert_eq!(
            decode_lcg(&zeros, 987_654_321, 3)[3..],
            [0; 5],
            "bytes past x-enc-len stay"
        );
    }

    #[test]
    fn algo_two_keeps_the_variant_that_looks_like_an_image() {
        let jpeg_start = [
            0xFF, 0xD8, 0xFF, 0xE0, 0, 16, b'J', b'F', b'I', b'F', 0, 1, 1,
        ];
        let seed = 4242;
        // Encode with the third candidate (xorshift of seed | 1, high byte); XOR is its own inverse.
        let encoded = decode_xorshift(&jpeg_start, seed | 1, 13, true);
        assert_eq!(
            decode_encoded_bytes(&encoded, seed, 13, Some("2")),
            jpeg_start
        );
        assert_eq!(
            decode_encoded_bytes(&encoded, seed, 13, Some("1")),
            decode_lcg(&encoded, seed, 13),
            "other algos use the LCG"
        );
    }

    #[test]
    fn plain_responses_are_left_alone() {
        let mut headers = HeaderMap::new();
        assert_eq!(unscramble(b"abc", &headers), Ok(None));
        headers.insert("x-enc-seed", "0".parse().unwrap());
        headers.insert("x-enc-len", "3".parse().unwrap());
        assert_eq!(
            unscramble(b"abc", &headers),
            Ok(None),
            "seed 0 means no XOR"
        );
        headers.insert("x-scramble-grid", "4x4".parse().unwrap());
        headers.insert("x-scramble-seed", "5".parse().unwrap());
        assert_eq!(unscramble(b"abc", &headers), Ok(None), "only 5x5 grids");
    }

    #[test]
    fn an_xor_only_body_comes_back_decoded() {
        let plain = b"\x89PNG\r\n\x1a\nrest".to_vec();
        let encoded = decode_lcg(&plain, 77, 6);
        let mut headers = HeaderMap::new();
        headers.insert("x-enc-seed", "77".parse().unwrap());
        headers.insert("x-enc-len", "6".parse().unwrap());
        assert_eq!(unscramble(&encoded, &headers), Ok(Some(plain)));
    }

    #[test]
    fn a_shuffled_grid_is_put_back() {
        // 50x50 with a flat colour per 10x10 tile; shuffle it the way the server does (dst tile from order[dst]).
        let colour =
            |t: usize| image::Rgb([(t * 10) as u8, 255 - (t * 9) as u8, ((t * 37) % 256) as u8]);
        let original =
            image::RgbImage::from_fn(50, 50, |x, y| colour((y / 10 * 5 + x / 10) as usize));
        let (seed, hash) = (31337, 58414);
        let order = tile_order(seed ^ hash, Some("2"));
        let mut shuffled = original.clone();
        for (dst, &src) in order.iter().enumerate() {
            for y in 0..10 {
                for x in 0..10 {
                    let (sx, sy) = ((src % 5) as u32 * 10 + x, (src / 5) as u32 * 10 + y);
                    let (dx, dy) = ((dst % 5) as u32 * 10 + x, (dst / 5) as u32 * 10 + y);
                    shuffled.put_pixel(sx, sy, *original.get_pixel(dx, dy));
                }
            }
        }
        assert_ne!(shuffled, original);
        assert_eq!(descramble(&shuffled, seed ^ hash, Some("2")), original);

        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(shuffled.as_raw(), 50, 50, image::ExtendedColorType::Rgb8)
            .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("x-scramble-grid", "5x5".parse().unwrap());
        headers.insert("x-scramble-seed", seed.to_string().parse().unwrap());
        headers.insert("x-scramble-algo", "2".parse().unwrap());
        headers.insert("x-scramble-hash", "03632".parse().unwrap());
        let jpeg = unscramble(&png, &headers).unwrap().expect("descrambled");
        let back = image::load_from_memory(&jpeg).unwrap().to_rgb8();
        for t in 0..TILES {
            let (x, y) = ((t % 5) as u32 * 10 + 5, (t / 5) as u32 * 10 + 5);
            let (got, want) = (back.get_pixel(x, y), colour(t));
            assert!(
                got.0.iter().zip(want.0).all(|(g, w)| g.abs_diff(w) <= 12),
                "tile {t}: {got:?} vs {want:?}"
            );
        }
        assert!(unscramble(b"not an image", &headers).is_err());
    }

    include!("../tests/fixtures/descramble_vectors.rs");
}
