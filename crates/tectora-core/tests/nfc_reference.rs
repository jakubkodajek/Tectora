//! Reference check of the NFC table (TEST-110): the decomposed form (NFD)
//! of every precomposed letter in the blocks `nfc.rs` covers composes back
//! to the letter. The pairs come from Python's `unicodedata`
//! (`tests/support/nfc_pairs.txt`), so no dependency is needed. Greek is a
//! European language too, and macOS hands Greek names over decomposed.

use tectora_core::nfc::nfc;

/// The pairs of the blocks whose names start with `prefix`.
fn pairs(prefix: &str) -> Vec<(String, char, String)> {
    let text = include_str!("support/nfc_pairs.txt");
    let hex = |h: &str| char::from_u32(u32::from_str_radix(h, 16).unwrap()).unwrap();
    let out: Vec<_> = text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| {
            let mut f = l.split_whitespace();
            let block = f.next()?.to_string();
            let letter = hex(f.next()?);
            let nfd: String = f.map(hex).collect();
            block.starts_with(prefix).then_some((block, letter, nfd))
        })
        .collect();
    assert!(!out.is_empty(), "no reference pairs for {prefix}");
    out
}

/// Every letter of the blocks whose NFD does not compose back, as
/// `block U+XXXX` with the letter.
fn wrong(prefix: &str) -> Vec<String> {
    pairs(prefix)
        .into_iter()
        .filter(|(_, letter, nfd)| nfc(nfd) != letter.to_string())
        .map(|(block, letter, _)| format!("{block} U+{:04X} {letter}", letter as u32))
        .collect()
}

/// NFD of a name, written as base letters and combining marks.
fn check_names(names: &[(&str, &str)]) {
    for (nfd, want) in names {
        assert_ne!(nfd, want, "the test name must be decomposed");
        assert_eq!(nfc(nfd), *want);
        assert_eq!(nfc(want), *want, "a precomposed name stays as it is");
    }
}

#[test]
fn every_decomposed_latin_letter_composes() {
    let bad = wrong("latin");
    assert!(bad.is_empty(), "{} wrong: {bad:?}", bad.len());
    check_names(&[
        ("Z\u{307}o\u{301}\u{142}c\u{301}", "Żółć"),
        ("Gro\u{308}\u{df}e", "Größe"),
        ("S\u{327}is\u{327}li", "Şişli"),
        ("Ma\u{306}gurele", "Măgurele"),
        ("A\u{328}z\u{30c}uolas", "Ąžuolas"),
        ("G\u{327}irts", "Ģirts"),
        ("N\u{303}andu\u{301}", "Ñandú"),
        ("W\u{302}y", "Ŵy"),
    ]);
}

#[test]
fn every_decomposed_cyrillic_letter_composes() {
    let bad = wrong("cyrillic");
    assert!(bad.is_empty(), "{} wrong: {bad:?}", bad.len());
    check_names(&[
        ("\u{418}\u{306}oshkar", "Йoshkar"),
        ("\u{415}\u{308}\u{43b}\u{43a}\u{430}", "Ёлка"),
        ("\u{406}\u{308}\u{436}\u{430}", "Їжа"),
        ("\u{41a}\u{301}", "Ќ"),
        ("\u{423}\u{306}", "Ў"),
    ]);
}

#[test]
fn every_decomposed_greek_letter_composes() {
    let bad = wrong("greek");
    assert!(bad.is_empty(), "{} wrong: {bad:?}", bad.len());
    check_names(&[
        (
            "\u{395}\u{3bb}\u{3bb}\u{3b7}\u{3bd}\u{3b9}\u{3ba}\u{3b1}\u{301}",
            "Ελληνικά",
        ),
        ("\u{391}\u{301}\u{3b8}\u{3b7}\u{3bd}\u{3b1}", "Άθηνα"),
        ("\u{3c9}\u{313}\u{342}\u{345}", "ᾦ"),
    ]);
}
