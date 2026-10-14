//! Dean Edwards' P.A.C.K.E.R. unpacker, `eval(function(p,a,c,k,e,d){...}('payload',radix,count,'a|b'.split('|'),0,{}))`.
//! Ported from `JsUnpacker` and `Unbaser` in lib/unpacker of the yuzono_anime-extensions reference tree (package
//! `keiyoushi.lib.jsunpacker`; Apache-2.0, as that repository's root LICENSE). The file names DatL4g/JsUnpacker as its origin;
//! that upstream's licence was not checked.
//! Like the reference, the payload is not JS-unescaped: `\'` stays `\'`.

use regex_lite::{Captures, Regex};

const ALPHABET_52: &str = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOP";
const ALPHABET_54: &str = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQR";
const ALPHABET_62: &str = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
// The 95 printable ASCII characters, as in the packer's JS source. The reference's Kotlin copy has one extra backslash
// (96 entries). Output differs from the reference for radix 63 to 95 (every character after '&' is one index lower here).
// Unverified whether Kwik uses such a radix; radix up to 62 is identical.
const ALPHABET_95: &str = r##" !"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\]^_`abcdefghijklmnopqrstuvwxyz{|}~"##;

/// True when the text holds a packed call.
pub fn detect(text: &str) -> bool {
    Regex::new(r"(?i)eval\(function\(p,a,c,k,e,[rd]?").unwrap().is_match(text)
}

/// Unpacks every packed call in `text` and joins the results with a space. None when nothing unpacks or the result is blank.
pub fn unpack_and_combine(text: &str) -> Option<String> {
    let out = unpack_all(text).join(" ");
    (!out.trim().is_empty()).then_some(out)
}

fn unpack_all(text: &str) -> Vec<String> {
    if !detect(text) {
        return Vec::new();
    }
    let extract = Regex::new(r"\}\s*\('(.*)',\s*([0-9]+),\s*([0-9]+),\s*'(.*?)'\.split\('\|'\)").unwrap();
    let word = Regex::new(r"[A-Za-z0-9_]+").unwrap();
    extract
        .captures_iter(text)
        .filter_map(|c| {
            let radix = c[2].parse::<i32>().unwrap_or(10);
            let count = c[3].parse::<usize>().ok()?;
            let symtab: Vec<&str> = c[4].split('|').collect();
            if symtab.len() != count {
                return None; // the reference ignores such a call: it cannot be unpacked correctly
            }
            let out = word.replace_all(&c[1], |m: &Captures| match symtab.get(unbase(&m[0], radix)) {
                Some(s) if !s.is_empty() => s.to_string(),
                _ => m[0].to_string(),
            });
            Some(out.into_owned())
        })
        .collect()
}

/// Reads a packed word as a number in `radix`. A word that does not parse reads as 0, as in the reference.
fn unbase(word: &str, radix: i32) -> usize {
    if (2..=36).contains(&radix) {
        return i32::from_str_radix(word, radix as u32).unwrap_or(0) as usize;
    }
    let alphabet = if radix > 62 {
        ALPHABET_95
    } else if radix > 54 {
        ALPHABET_62
    } else if radix > 52 {
        ALPHABET_54
    } else {
        ALPHABET_52
    };
    let base = radix as u64;
    let mut value: u64 = 0;
    for (i, c) in word.chars().rev().enumerate() {
        let digit = alphabet.find(c).unwrap_or(0) as u64;
        value = value.wrapping_add(base.wrapping_pow(i as u32).wrapping_mul(digit));
    }
    value as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    // The two packed calls of the reference unit test (lib/unpacker, JsUnpackerUnitTest.kt), text unescaped from Kotlin.
    const PACKED_CALL: &str = r#"eval(function(p,a,c,k,e,r){e=String;if(!''.replace(/^/,String)){while(c--)r[c]=k[c]||c;k=[function(e){return r[e]}];e=function(){return'\\w+'};c=1};while(c--)if(k[c])p=p.replace(new RegExp('\\b'+e(c)+'\\b','g'),k[c]);return p}('0(\'1 2 3 4 5 6 7\');',8,8,'alert|This|is|packed|and|a|plain|call'.split('|'),0,{}))"#;
    const PACKED_FUNCTION: &str = r#"eval(function(p,a,c,k,e,r){e=String;if(!''.replace(/^/,String)){while(c--)r[c]=k[c]||c;k=[function(e){return r[e]}];e=function(){return'\\w+'};c=1};while(c--)if(k[c])p=p.replace(new RegExp('\\b'+e(c)+'\\b','g'),k[c]);return p}('0 1(){2(\'3 4 5 6 7 0\')}',8,8,'function|funPackedTest|alert|This|is|packed|and|a'.split('|'),0,{}))"#;

    #[test]
    fn unpacks_the_reference_calls() {
        // The reference accepts the unescaped text too; this port keeps the escapes, so the second form is expected.
        assert_eq!(unpack_and_combine(PACKED_CALL).as_deref(), Some(r"alert(\'This is packed and a plain call\');"));
        assert_eq!(
            unpack_and_combine(PACKED_FUNCTION).as_deref(),
            Some(r"function funPackedTest(){alert(\'This is packed and a function\')}"),
        );
    }

    #[test]
    fn detects_packed_text_only() {
        assert!(detect(PACKED_CALL));
        assert!(detect("eval(function(p,a,c,k,e,d){return p}"));
        assert!(!detect("alert('This is not packed and a plain call');"));
        assert_eq!(unpack_and_combine("alert('plain');"), None);
    }

    #[test]
    fn symtab_count_mismatch_is_ignored() {
        let bad = PACKED_CALL.replace(",8,8,", ",8,9,");
        assert_eq!(unpack_and_combine(&bad), None);
    }

    #[test]
    fn unbase_reads_each_radix_range() {
        assert_eq!(unbase("10", 8), 8);
        assert_eq!(unbase("zz", 36), 1295);
        assert_eq!(unbase("bad!", 10), 0);
        assert_eq!(unbase("10", 62), 62);
        assert_eq!(unbase("A", 62), 36);
        assert_eq!(unbase("10", 95), 1631); // "0" is index 16 and "1" index 17 of the 95-char alphabet
        assert_eq!(ALPHABET_95.chars().count(), 95);
        assert!(ALPHABET_95.chars().eq((32u8..=126).map(char::from)));
    }
}
