//! Searching and sorting the job library (the roofs made so far), and
//! starting a new job from one of them.
//!
//! The app filters the summaries it already holds in the UI; the MCP server
//! and the CLI use [`search`] so an AI client can do the same: free text,
//! status, last change by an AI, date range, sort order and a limit.

use serde::Deserialize;

use crate::model::{JobInfo, JobStatus, Project};
use crate::store::ProjectSummary;

/// What to sort the library by.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortKey {
    #[default]
    Modified,
    Name,
    Number,
    Customer,
    Area,
}

/// A library search. Every set field must match.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Query {
    /// Words that must all appear in the name, job number, customer,
    /// address or notes (case and accents ignored).
    pub text: String,
    pub status: Option<JobStatus>,
    /// Only jobs whose last recorded change was made by an AI client.
    pub by_ai: bool,
    /// Last changed on or after this day (`YYYY-MM-DD`, UTC).
    pub modified_from: Option<String>,
    /// Last changed on or before this day (`YYYY-MM-DD`, UTC).
    pub modified_to: Option<String>,
    /// Roof area at least / at most this many m².
    pub min_area: Option<f64>,
    pub max_area: Option<f64>,
    pub sort: SortKey,
    /// Descending order; defaults to newest / largest first for `modified`
    /// and `area`, A to Z otherwise.
    pub desc: Option<bool>,
    pub limit: Option<usize>,
}

const DAY_MS: u64 = 86_400_000;

/// Filters and sorts `list` by `q`. Fails only on a malformed date.
pub fn search(list: Vec<ProjectSummary>, q: &Query) -> Result<Vec<ProjectSummary>, String> {
    let from = q.modified_from.as_deref().map(parse_day).transpose()?;
    let to = q
        .modified_to
        .as_deref()
        .map(parse_day)
        .transpose()?
        .map(|d| d + DAY_MS);
    let words = search_words(&fold(&q.text));
    let mut out: Vec<ProjectSummary> = list
        .into_iter()
        .filter(|p| {
            q.status.is_none_or(|s| p.job.status == s)
                // One rule with the app's filter, count and badge
                // (`store::changed_by_ai`, audit 2026-11-22 UX-806).
                && (!q.by_ai || p.changed_by_ai.is_some())
                && from.is_none_or(|f| p.modified_ms >= f)
                && to.is_none_or(|t| p.modified_ms < t)
                // An area not known yet (ARCH-001) hides no job.
                && (p.figures_pending || q.min_area.is_none_or(|a| p.roof_area >= a))
                && (p.figures_pending || q.max_area.is_none_or(|a| p.roof_area <= a))
                && matches_words(p, &words)
        })
        .collect();
    let desc = q
        .desc
        .unwrap_or(matches!(q.sort, SortKey::Modified | SortKey::Area));
    out.sort_by(|a, b| {
        let ord = match q.sort {
            SortKey::Modified => a.modified_ms.cmp(&b.modified_ms),
            SortKey::Area => a.roof_area.total_cmp(&b.roof_area),
            SortKey::Name => fold(&a.name).cmp(&fold(&b.name)),
            SortKey::Number => fold(&a.job.number).cmp(&fold(&b.job.number)),
            SortKey::Customer => fold(&a.job.customer).cmp(&fold(&b.job.customer)),
        };
        if desc { ord.reverse() } else { ord }
    });
    if let Some(n) = q.limit {
        out.truncate(n);
    }
    Ok(out)
}

/// The words of the folded search text `folded`: split at white space
/// (Unicode `White_Space`, as `split_whitespace`) and at U+FEFF, which the
/// app's `\s` splits at too; `searchWords` in `app/src/lib/fold.ts` splits
/// the same (audit 2026-11-22 TEST-608).
pub fn search_words(folded: &str) -> Vec<String> {
    folded
        .split(|c: char| c.is_whitespace() || c == '\u{feff}')
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

fn matches_words(p: &ProjectSummary, words: &[String]) -> bool {
    if words.is_empty() {
        return true;
    }
    let hay = fold(
        &[
            p.name.as_str(),
            &p.job.number,
            &p.job.customer,
            &p.job.address,
            &p.notes,
        ]
        .join("\n"),
    );
    words.iter().all(|w| hay.contains(w.as_str()))
}

/// Lower case without the accents of the Latin and Greek letters, for
/// search, sorting and job ids ("Říčany" matches "ricany", "ΣΠΊΤΙ" matches
/// "σπίτι"), ligatures and `ß` spelled out ("ﬁle" is "file", "Straße"
/// "strasse"), composed first so a name typed or stored decomposed (macOS)
/// matches as well (HUNTIO-007, audit 2026-11-21 HUNT-204 item 3). Every
/// other letter with marks the composition table knows loses them too
/// (`ȧ`, polytonic Greek `ὀ`), a mark left over is dropped (`İ` lower-cases
/// to `i` and a dot above), and full-width letters and digits are the
/// plain ones (`ＡＢＣ` is `abc`; audit 2026-11-22 HUNT-305 item 1).
pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in crate::nfc::nfc(s).to_lowercase().chars() {
        if ('\u{300}'..='\u{36f}').contains(&c) {
            continue;
        }
        let c = match c {
            '\u{ff01}'..='\u{ff5e}' => char::from_u32(c as u32 - 0xfee0).unwrap_or(c),
            c => c,
        };
        fold_into(c, &mut out);
    }
    out
}

/// Pushes the folded form of the lower-case letter `c` onto `out`.
pub(crate) fn fold_into(c: char, out: &mut String) {
    let spelled = match c {
        'ß' => "ss",
        'æ' => "ae",
        'œ' => "oe",
        'þ' => "th",
        '\u{fb00}' => "ff",
        '\u{fb01}' => "fi",
        '\u{fb02}' => "fl",
        '\u{fb03}' => "ffi",
        '\u{fb04}' => "ffl",
        '\u{fb05}' | '\u{fb06}' => "st",
        _ => "",
    };
    if !spelled.is_empty() {
        out.push_str(spelled);
        return;
    }
    // The base of a letter with marks is folded again: `ǽ` is `æ`, spelled
    // `ae`, and `ǿ` is `ø`, which is `o`, as the app folds them (audit
    // 2026-11-22 HUNT-602).
    let mut c = c;
    for _ in 0..4 {
        let base = fold_char(c);
        if base == c {
            break;
        }
        if matches!(base, 'ß' | 'æ' | 'œ' | 'þ' | '\u{fb00}'..='\u{fb06}') {
            return fold_into(base, out);
        }
        c = base;
    }
    out.push(c);
}

fn fold_char(c: char) -> char {
    match c {
        'á' | 'ä' | 'à' | 'â' | 'ã' | 'å' | 'ā' | 'ă' | 'ą' | '\u{212b}' => 'a',
        'č' | 'ç' | 'ć' | 'ĉ' | 'ċ' => 'c',
        'ď' | 'đ' | 'ð' => 'd',
        'é' | 'ě' | 'è' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' => 'e',
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => 'g',
        'ĥ' | 'ħ' => 'h',
        'í' | 'î' | 'ì' | 'ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' => 'i',
        'ĵ' => 'j',
        'ķ' => 'k',
        'ĺ' | 'ľ' | 'ļ' | 'ŀ' | 'ł' => 'l',
        'ň' | 'ñ' | 'ń' | 'ņ' => 'n',
        'ó' | 'ô' | 'ö' | 'ò' | 'õ' | 'ø' | 'ō' | 'ŏ' | 'ő' => 'o',
        'ŕ' | 'ř' | 'ŗ' => 'r',
        'š' | 'ś' | 'ŝ' | 'ş' => 's',
        'ť' | 'ţ' | 'ŧ' => 't',
        'ú' | 'ů' | 'ü' | 'ù' | 'û' | 'ũ' | 'ū' | 'ŭ' | 'ű' | 'ų' => 'u',
        'ŵ' => 'w',
        'ý' | 'ÿ' | 'ŷ' => 'y',
        'ž' | 'ź' | 'ż' => 'z',
        // Greek: the accents (tonos, dialytika) and the final sigma.
        'ά' => 'α',
        'έ' => 'ε',
        'ή' => 'η',
        'ί' | 'ϊ' | 'ΐ' => 'ι',
        'ό' => 'ο',
        'ύ' | 'ϋ' | 'ΰ' => 'υ',
        'ώ' => 'ω',
        'ς' => 'σ',
        c => crate::nfc::base_letter(c).unwrap_or(c),
    }
}

/// Midnight UTC of `YYYY-MM-DD`, in ms since 1970.
fn parse_day(s: &str) -> Result<u64, String> {
    let bad = || format!("invalid date `{s}`, expected YYYY-MM-DD");
    let mut it = s.trim().splitn(3, '-');
    let mut num = || {
        it.next()
            .and_then(|p| p.parse::<i64>().ok())
            .ok_or_else(bad)
    };
    let (y, m, d) = (num()?, num()?, num()?);
    if !(1970..=9999).contains(&y) || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return Err(bad());
    }
    // Days from civil (H. Hinnant, "chrono-Compatible Low-Level Date
    // Algorithms", https://howardhinnant.github.io/date_algorithms.html).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Ok(days as u64 * DAY_MS)
}

/// Turns a copy of a past job into a new job: new name, the given job data
/// (or the old one), always a draft and without the stored offer result.
pub fn prepare_copy(project: &mut Project, name: &str, job: Option<JobInfo>) {
    project.name = name.to_string();
    if let Some(job) = job {
        project.job = job;
    }
    project.job.status = JobStatus::Draft;
    project.result_snapshot = None;
}

/// Lowest and highest pitch of the roofed edges (gables left out), degrees.
pub fn pitch_range(project: &Project) -> Option<[f64; 2]> {
    project
        .parts
        .iter()
        .flat_map(|p| &p.edges)
        .map(|e| e.pitch_deg)
        .filter(|p| *p > 0.0 && *p < 90.0)
        .fold(None, |r, p| match r {
            None => Some([p, p]),
            Some([lo, hi]) => Some([lo.min(p), hi.max(p)]),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::ActivityInfo;

    fn summary(id: &str, name: &str, customer: &str, ms: u64, area: f64) -> ProjectSummary {
        ProjectSummary {
            id: id.into(),
            name: name.into(),
            notes: String::new(),
            job: JobInfo {
                customer: customer.into(),
                ..Default::default()
            },
            modified_ms: ms,
            roof_area: area,
            parts: 1,
            outlines: vec![],
            lines: vec![],
            last_activity: None,
            changed_by_ai: None,
            pitch_range: None,
            dormers: 0,
            files: 0,
            has_result: false,
            figures_pending: false,
            parts_failed: 0,
            stored_area: None,
            result_differs: false,
            too_complex: false,
            refused: false,
            faces: 0,
        }
    }

    fn ids(v: &[ProjectSummary]) -> Vec<&str> {
        v.iter().map(|p| p.id.as_str()).collect()
    }

    #[test]
    fn dates_are_utc_midnight() {
        assert_eq!(parse_day("1970-01-01"), Ok(0));
        assert_eq!(parse_day("2000-03-01"), Ok(951_868_800_000));
        assert_eq!(parse_day("2026-11-03"), Ok(1_793_664_000_000));
        assert!(parse_day("2026-13-01").is_err());
        assert!(parse_day("yesterday").is_err());
    }

    #[test]
    fn text_ignores_case_and_accents_and_needs_every_word() {
        let list = vec![
            summary("a", "Rodinný dům", "Novák", 1, 100.0),
            summary("b", "Garáž", "Nováková", 2, 30.0),
            summary("c", "Hala", "Dvořák", 3, 500.0),
        ];
        let q = |text: &str| Query {
            text: text.into(),
            ..Default::default()
        };
        assert_eq!(ids(&search(list.clone(), &q("NOVAK")).unwrap()), ["b", "a"]);
        assert_eq!(ids(&search(list.clone(), &q("novak dum")).unwrap()), ["a"]);
        assert_eq!(ids(&search(list.clone(), &q("dvorak")).unwrap()), ["c"]);
        assert!(search(list, &q("novak hala")).unwrap().is_empty());
    }

    /// Audit 2026-11-21 HUNT-204 item 3: Greek is accent-blind as Czech
    /// is, and ligatures and `ß` match their spelled-out letters.
    #[test]
    fn greek_accents_ligatures_and_sharp_s_are_folded() {
        let list = vec![
            summary("a", "Ελληνικό σπίτι", "", 1, 100.0),
            summary("b", "ΕΛΛΗΝΙΚΟ ΣΠΙΤΙ", "", 2, 30.0),
            summary("c", "Straße ﬁle", "", 3, 30.0),
        ];
        let q = |text: &str| Query {
            text: text.into(),
            ..Default::default()
        };
        assert_eq!(ids(&search(list.clone(), &q("σπίτι")).unwrap()), ["b", "a"]);
        assert_eq!(
            ids(&search(list.clone(), &q("ελληνικος")).unwrap()),
            Vec::<&str>::new()
        );
        assert_eq!(
            ids(&search(list.clone(), &q("strasse file")).unwrap()),
            ["c"]
        );
        assert_eq!(ids(&search(list, &q("STRAẞE")).unwrap()), ["c"]);
    }

    /// Audit 2026-11-22 TEST-306 R8: a capital sigma at a word's end is
    /// lower-cased as the final sigma, which is found by the plain one.
    #[test]
    fn a_final_sigma_is_found_by_the_plain_sigma() {
        let list = vec![summary("a", "ΣΠΊΤΙ ΝΊΚΟΣ", "", 1, 1.0)];
        let q = |text: &str| Query {
            text: text.into(),
            ..Default::default()
        };
        assert_eq!(ids(&search(list.clone(), &q("νικοσ")).unwrap()), ["a"]);
        assert_eq!(ids(&search(list, &q("νίκος")).unwrap()), ["a"]);
        assert_eq!(
            crate::store::slug_of("ΣΠΊΤΙ ΝΊΚΟΣ"),
            crate::store::slug_of("σπιτι νικοσ")
        );
    }

    /// Audit 2026-11-22 HUNT-305 item 1: a dotted capital I, a letter with
    /// a dot above, full-width letters and polytonic Greek are folded too.
    #[test]
    fn dotted_full_width_and_polytonic_letters_are_folded() {
        let list = vec![
            summary("a", "İstanbul", "", 1, 1.0),
            summary("b", "ȧb", "", 2, 1.0),
            summary("c", "ＡＢＣ dům", "", 3, 1.0),
            summary("d", "Ὀδυσσεύς", "", 4, 1.0),
        ];
        let q = |text: &str| Query {
            text: text.into(),
            ..Default::default()
        };
        assert_eq!(ids(&search(list.clone(), &q("istanbul")).unwrap()), ["a"]);
        assert_eq!(ids(&search(list.clone(), &q("ab")).unwrap()), ["c", "b"]);
        assert_eq!(ids(&search(list.clone(), &q("abc dum")).unwrap()), ["c"]);
        assert_eq!(ids(&search(list, &q("οδυσσευς")).unwrap()), ["d"]);
        assert_eq!(fold("İstanbul"), "istanbul");
        assert_eq!(crate::store::slug_of("İstanbul"), "istanbul");
        assert_eq!(crate::store::slug_of("ȧb"), "ab");
        assert_eq!(crate::store::slug_of("ＡＢＣ dům"), "abc-dum");
    }

    /// Audit 2026-11-22 UX-503: the core folds as the app's library search
    /// does (`app/src/lib/fold.ts`, tested with the same cases): "m2" found
    /// "120 m²" in the app but not over MCP.
    #[test]
    fn fold_matches_the_shared_cases() {
        let text = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/parity/fold_cases.json"),
        )
        .unwrap();
        let file: serde_json::Value = serde_json::from_str(&text).unwrap();
        let cases = file["cases"].as_array().unwrap();
        assert!(cases.len() > 10);
        for c in cases {
            let (input, folded) = (c[0].as_str().unwrap(), c[1].as_str().unwrap());
            assert_eq!(fold(input), folded, "{input}");
        }
    }

    /// The inputs of `tests/fixtures/parity/fold_all.json`, made the same
    /// way by `app/src/lib/fold.test.ts`: every code point from U+0020 to
    /// U+2FFFF (no surrogates), letters with each mark U+0300 to U+036F,
    /// letters with two marks, letters with marks of other ranges, the
    /// decomposed sequences of `nfc_inputs.json` and Hangul jamo.
    fn parity_inputs() -> Vec<String> {
        let mut out: Vec<String> = (0x20..0x30000)
            .filter_map(char::from_u32)
            .map(String::from)
            .collect();
        let marks = || (0x300..0x370).filter_map(char::from_u32);
        for l in "aeiouyAEIOUYcnszrdtlgkhwjCNSZRDTLGKHWJαεηιουωΑΕΗΙΟΥΩæøÆØ".chars()
        {
            out.extend(marks().map(|m| format!("{l}{m}")));
        }
        for l in "aeoAEOuUαω".chars() {
            for m in marks() {
                for m2 in [0x301, 0x302, 0x303, 0x308, 0x313, 0x323, 0x328, 0x342] {
                    out.push(format!("{l}{m}{}", char::from_u32(m2).unwrap()));
                }
            }
        }
        for l in "aAo".chars() {
            for m in [
                0x1ab0, 0x1dc0, 0x20d0, 0xfe20, 0x0483, 0x0591, 0x064b, 0x093c, 0x302a, 0x3099,
            ] {
                out.push(format!("{l}{}", char::from_u32(m).unwrap()));
            }
        }
        // Audit 2026-11-22 TEST-710: decomposed sequences with marks outside
        // U+0300-036F (tests/fixtures/parity/nfc_inputs.json) and Hangul
        // jamo: every leading consonant + vowel, with four trailing
        // consonants, and a syllable + every trailing consonant.
        let text = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/parity/nfc_inputs.json"),
        )
        .unwrap();
        let file: serde_json::Value = serde_json::from_str(&text).unwrap();
        out.extend(
            file["inputs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string()),
        );
        let ch = |c: u32| char::from_u32(c).unwrap();
        for l in 0x1100..=0x1112 {
            for v in 0x1161..=0x1175 {
                out.push(format!("{}{}", ch(l), ch(v)));
                for t in [0x11a8, 0x11ab, 0x11af, 0x11c2] {
                    out.push(format!("{}{}{}", ch(l), ch(v), ch(t)));
                }
            }
        }
        for t in 0x11a7..=0x11c3 {
            out.push(format!("\u{ac00}{}", ch(t)));
            out.push(format!("\u{ac01}{}", ch(t)));
        }
        out
    }

    /// Audit 2026-11-22 HUNT-602 / TEST-608: the core's fold over every
    /// code point and many letter + mark sequences is the one in
    /// `tests/fixtures/parity/fold_all.json` (only the inputs it changes),
    /// which `app/src/lib/fold.test.ts` holds the app's fold to, so the
    /// library search and MCP `list_projects` fold every text alike. Write
    /// it again with `TECTORA_WRITE_FOLD=1` after changing the fold.
    #[test]
    fn fold_matches_the_generated_parity_file() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/parity/fold_all.json");
        let changed: std::collections::BTreeMap<String, String> = parity_inputs()
            .into_iter()
            .filter_map(|i| {
                let f = fold(&i);
                (f != i).then_some((i, f))
            })
            .collect();
        let words: Vec<(String, Vec<String>)> = [
            "a\u{feff}b",
            "a\u{85}b",
            "a\u{a0}b\u{2003}c\u{3000}d",
            " a\tb\nc\u{2028}d\u{200b}e ",
            "a\u{1680}b\u{180e}c\u{202f}d\u{205f}e",
        ]
        .iter()
        .map(|s| (s.to_string(), search_words(&fold(s))))
        .collect();
        let file = serde_json::json!({
            "_comment": "Generated by the core test library::tests::fold_matches_the_generated_parity_file (TECTORA_WRITE_FOLD=1); app/src/lib/fold.test.ts makes the same inputs and holds fold.ts to it. `changed`: every input the fold changes, with its folded form; every other input folds to itself. `words`: a folded text and the search words it splits into.",
            "changed": changed,
            "words": words,
        });
        if std::env::var_os("TECTORA_WRITE_FOLD").is_some() {
            std::fs::write(&path, serde_json::to_string(&file).unwrap() + "\n").unwrap();
            return;
        }
        let stored: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let differ: Vec<_> = changed
            .iter()
            .filter(|(i, f)| stored["changed"][i.as_str()].as_str() != Some(f.as_str()))
            .take(10)
            .collect();
        assert!(
            differ.is_empty(),
            "fold changed, rewrite the file: {differ:?}"
        );
        assert_eq!(stored, file, "rewrite with TECTORA_WRITE_FOLD=1");
    }

    #[test]
    fn a_decomposed_name_from_macos_is_found_by_its_precomposed_spelling() {
        // HUNTIO-007: "Nový Dům" typed on macOS (NFD) was not found by
        // "Dům" or "dum", and the other way round.
        let list = vec![
            summary("a", "Nov\u{79}\u{301} D\u{75}\u{30a}m", "", 1, 1.0),
            summary("b", "Dům", "", 2, 1.0),
        ];
        let q = |text: &str| Query {
            text: text.into(),
            ..Default::default()
        };
        assert_eq!(ids(&search(list.clone(), &q("Dům")).unwrap()), ["b", "a"]);
        assert_eq!(ids(&search(list.clone(), &q("dum")).unwrap()), ["b", "a"]);
        assert_eq!(
            ids(&search(list, &q("D\u{75}\u{30a}m")).unwrap()),
            ["b", "a"]
        );
    }

    #[test]
    fn filters_sorts_and_limits() {
        let day = DAY_MS;
        let mut list = vec![
            summary(
                "a",
                "b-name",
                "",
                parse_day("2025-06-01").unwrap() + 5,
                100.0,
            ),
            summary("b", "a-name", "", parse_day("2026-01-01").unwrap(), 30.0),
            summary(
                "c",
                "č-name",
                "",
                parse_day("2026-02-01").unwrap() + day - 1,
                500.0,
            ),
        ];
        list[2].job.status = JobStatus::Done;
        list[1].last_activity = Some(ActivityInfo {
            ts_ms: 0,
            actor: "claude".into(),
            summary: String::new(),
            kind: None,
        });
        // As the store fills it for a job an AI changed last
        // (`store::changed_by_ai`), which is what `by_ai` reads (UX-806).
        list[1].changed_by_ai = list[1].last_activity.clone();
        let run = |q: Query| ids(&search(list.clone(), &q).unwrap()).join(",");
        assert_eq!(run(Query::default()), "c,b,a");
        assert_eq!(
            run(Query {
                sort: SortKey::Name,
                ..Default::default()
            }),
            "b,a,c"
        );
        assert_eq!(
            run(Query {
                sort: SortKey::Area,
                limit: Some(2),
                ..Default::default()
            }),
            "c,a"
        );
        assert_eq!(
            run(Query {
                status: Some(JobStatus::Done),
                ..Default::default()
            }),
            "c"
        );
        assert_eq!(
            run(Query {
                by_ai: true,
                ..Default::default()
            }),
            "b"
        );
        assert_eq!(
            run(Query {
                modified_from: Some("2026-01-01".into()),
                modified_to: Some("2026-02-01".into()),
                ..Default::default()
            }),
            "c,b"
        );
        assert_eq!(
            run(Query {
                min_area: Some(50.0),
                max_area: Some(200.0),
                ..Default::default()
            }),
            "a"
        );
    }

    #[test]
    fn a_job_whose_area_is_not_known_yet_stays_in_an_area_search() {
        // ARCH-001: a roof still calculated in the background is listed
        // with area 0; an area filter must not hide it.
        let mut slow = summary("slow", "Slow", "", 1, 0.0);
        slow.figures_pending = true;
        let small = summary("small", "Small", "", 2, 10.0);
        let q = Query {
            min_area: Some(50.0),
            ..Default::default()
        };
        assert_eq!(ids(&search(vec![slow, small], &q).unwrap()), ["slow"]);
    }

    #[test]
    fn copy_becomes_a_draft_with_new_job_data() {
        let mut p = crate::examples::find("gable").unwrap().project();
        p.job.status = JobStatus::Done;
        p.job.customer = "Old".into();
        prepare_copy(&mut p, "New", None);
        assert_eq!((p.name.as_str(), p.job.customer.as_str()), ("New", "Old"));
        assert_eq!(p.job.status, JobStatus::Draft);
        let job = JobInfo {
            customer: "Fresh".into(),
            status: JobStatus::Offer,
            ..Default::default()
        };
        prepare_copy(&mut p, "New", Some(job));
        assert_eq!(p.job.customer, "Fresh");
        assert_eq!(p.job.status, JobStatus::Draft);
        assert!(p.result_snapshot.is_none());
    }

    #[test]
    fn pitch_range_skips_gables() {
        let p = crate::examples::find("gable").unwrap().project();
        let [lo, hi] = pitch_range(&p).unwrap();
        assert!(lo > 0.0 && hi < 90.0 && lo <= hi);
    }
}
