//! Every code the core hands to the UI has a text in every language
//! (TEST-003). The UI builds these translation keys at run time
//! (`res.warn.${kind}`, `calc.err.${kind}`, `prob.${code}`, `err.${code}`,
//! `settings.err.${code}`), so the i18n test "every locale has the Czech
//! keys" cannot see a missing one: a new warning kind, calculation error,
//! input problem or error code in Rust would show a raw key and pass every
//! other test. The UI's hand-kept unions in `types.ts` and the `KNOWN` set
//! of `errors.ts` must list the same codes as the core.
//!
//! The enums are listed through exhaustive matches, so a new variant stops
//! this file from compiling until it is listed here (and then its keys are
//! checked); the `&'static str` codes are read from the sources.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use tectora_core::EdgeDef;
use tectora_core::check::WarningKind;
use tectora_core::model::ProblemItem;
use tectora_core::roof::CalcErrorKind;

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
const LOCALES: [&str; 4] = ["cs", "sk", "en", "de"];

fn read(rel: &str) -> String {
    let p = Path::new(ROOT).join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// The serde name (`snake_case`) of a unit variant.
fn name<T: serde::Serialize>(v: T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .expect("a unit variant serializes as a string")
}

fn warning_kinds() -> Vec<String> {
    use WarningKind as W;
    // A new variant fails to compile here: list it, then give it its texts.
    let all = |k: W| match k {
        W::OutsideOutline
        | W::OverlappingFaces
        | W::AreaMismatch
        | W::MergedWalls
        | W::Covered
        | W::CutExceedsFace
        | W::DormerOffRoof
        | W::WindowOffRoof
        | W::ChimneyOffRoof
        | W::OpeningOffRoof
        | W::NotCalculatedByCore
        | W::PlaneOverWall
        | W::MergedPlanes
        | W::WingJoined
        | W::DoubledLine
        | W::FarFromOrigin
        | W::NoRoofArea
        | W::ObjectsOverlap
        | W::DormerAboveRoof
        | W::InsideAnother
        | W::MergedMaterials
        | W::HiddenUnderNeighbour
        | W::UncoveredPlan
        | W::ManualFaceCrosses
        | W::ManualFacesOverlap
        | W::ManualFaceOverRoof
        | W::TowerBaseBelowRoof => k,
    };
    [
        W::OutsideOutline,
        W::OverlappingFaces,
        W::AreaMismatch,
        W::MergedWalls,
        W::Covered,
        W::CutExceedsFace,
        W::DormerOffRoof,
        W::WindowOffRoof,
        W::ChimneyOffRoof,
        W::OpeningOffRoof,
        W::NotCalculatedByCore,
        W::PlaneOverWall,
        W::MergedPlanes,
        W::WingJoined,
        W::DoubledLine,
        W::FarFromOrigin,
        W::NoRoofArea,
        W::ObjectsOverlap,
        W::DormerAboveRoof,
        W::InsideAnother,
        W::MergedMaterials,
        W::HiddenUnderNeighbour,
        W::UncoveredPlan,
        W::ManualFaceCrosses,
        W::ManualFacesOverlap,
        W::ManualFaceOverRoof,
        W::TowerBaseBelowRoof,
    ]
    .into_iter()
    .map(|k| name(all(k)))
    .collect()
}

fn calc_error_kinds() -> Vec<String> {
    use CalcErrorKind as K;
    let all = |k: K| match k {
        K::OpenFaces | K::NoSolution | K::NonConvexOpening | K::Cut | K::Geometry | K::NoEnd => k,
    };
    [
        K::OpenFaces,
        K::NoSolution,
        K::NonConvexOpening,
        K::Cut,
        K::Geometry,
        K::NoEnd,
    ]
    .into_iter()
    .map(|k| name(all(k)))
    .collect()
}

fn problem_items() -> Vec<String> {
    use ProblemItem as I;
    let all = |k: I| match k {
        I::Part | I::Dormer | I::Chimney | I::Opening | I::Window | I::Tower => k,
    };
    [
        I::Part,
        I::Dormer,
        I::Chimney,
        I::Opening,
        I::Window,
        I::Tower,
    ]
    .into_iter()
    .map(|k| name(all(k)))
    .collect()
}

/// The text from `at` to the brace that closes the first `{` after it.
fn body<'a>(src: &'a str, at: &str) -> &'a str {
    let start = src.find(at).unwrap_or_else(|| panic!("`{at}` not found"));
    let open = start + src[start..].find('{').expect("an opening brace");
    let mut depth = 0;
    for (i, ch) in src[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &src[open..open + i + 1];
                }
            }
            _ => {}
        }
    }
    panic!("no closing brace for `{at}`")
}

/// The `snake_case` string literals in `text` (`"invalid_id"`), skipping
/// comments.
fn literals(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in text.lines() {
        let code = line.split("//").next().unwrap_or("");
        for (i, piece) in code.split('"').enumerate() {
            if i % 2 == 1
                && !piece.is_empty()
                && piece.chars().all(|c| c.is_ascii_lowercase() || c == '_')
            {
                out.insert(piece.to_string());
            }
        }
    }
    out
}

/// The literals right after `marker` (`code: "x"`).
fn after(text: &str, marker: &str) -> BTreeSet<String> {
    text.match_indices(marker)
        .filter_map(|(i, _)| {
            let rest = text[i + marker.len()..].strip_prefix('"')?;
            Some(rest[..rest.find('"')?].to_string())
        })
        .collect()
}

/// Every input problem code the core reports.
fn problem_codes() -> BTreeSet<String> {
    let model = read("crates/tectora-core/src/model.rs");
    let problems = body(&model, "pub fn problems(&self) -> Vec<Problem>");
    let mut codes: BTreeSet<String> = EdgeDef::PROBLEMS
        .iter()
        .chain(tectora_core::tower::Tower::PROBLEMS.iter())
        .map(|(c, _)| c.to_string())
        .collect();
    for line in problems.lines().filter(|l| l.contains("Problem::of(")) {
        codes.extend(literals(line));
    }
    codes.extend(after(problems, "code: "));
    assert!(
        codes.contains("zero_length_wall") && codes.contains("bad_size"),
        "{codes:?}"
    );
    codes
}

/// The error codes the core's commands, store and AI-app setup return
/// (`err.<code>` in the UI).
fn error_codes() -> BTreeSet<String> {
    let mut codes = BTreeSet::new();
    let store = read("crates/tectora-core/src/store.rs");
    codes.extend(literals(body(&store, "pub fn code(&self) -> &'static str")));
    let assistants = read("crates/tectora-core/src/assistants.rs");
    codes.extend(literals(body(
        &assistants,
        "pub fn code(&self) -> &'static str",
    )));
    codes.extend(after(
        &read("crates/tectora-core/src/commands.rs"),
        "code: ",
    ));
    let model = read("crates/tectora-core/src/model.rs");
    codes.extend(after(body(&model, "pub fn check_input(&self)"), "code: "));
    for c in [
        "invalid_id",
        "revert_conflict",
        "ai_not_installed",
        "too_many",
        "not_calculated",
        "internal",
    ] {
        assert!(
            codes.contains(c),
            "{c} not read from the sources: {codes:?}"
        );
    }
    codes
}

fn settings_codes() -> BTreeSet<String> {
    let settings = read("crates/tectora-core/src/settings.rs");
    let codes = literals(body(&settings, "pub fn code(&self) -> &'static str"));
    assert!(codes.contains("not_writable"), "{codes:?}");
    codes
}

/// The keys of each locale's dictionary in `i18n.svelte.ts`.
fn dictionaries() -> BTreeMap<&'static str, BTreeSet<String>> {
    let src = read("app/src/lib/i18n.svelte.ts");
    LOCALES
        .iter()
        .map(|&loc| {
            let start = src
                .find(&format!("const {loc}: Dict = {{"))
                .unwrap_or_else(|| panic!("dictionary {loc}"));
            let end = start + src[start..].find("\n};").expect("end of the dictionary");
            let keys = src[start..end]
                .lines()
                .filter_map(|l| {
                    let l = l.trim_start().strip_prefix('"')?;
                    let k = &l[..l.find('"')?];
                    l[k.len() + 1..]
                        .trim_start()
                        .starts_with(':')
                        .then(|| k.to_string())
                })
                .collect::<BTreeSet<_>>();
            assert!(keys.len() > 100, "{loc}: only {} keys read", keys.len());
            (loc, keys)
        })
        .collect()
}

fn missing(dicts: &BTreeMap<&str, BTreeSet<String>>, keys: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for (loc, have) in dicts {
        for k in keys {
            if !have.contains(k) {
                out.push(format!("{loc}: {k}"));
            }
        }
    }
    out
}

#[test]
fn every_warning_error_and_problem_code_has_a_text_in_every_language() {
    let dicts = dictionaries();
    let mut keys = Vec::new();
    keys.extend(warning_kinds().iter().map(|k| format!("res.warn.{k}")));
    for k in calc_error_kinds() {
        keys.push(format!("calc.err.{k}"));
        keys.push(format!("calc.hint.{k}"));
    }
    keys.extend(problem_items().iter().map(|k| format!("prob.item.{k}")));
    keys.extend(problem_codes().iter().map(|k| format!("prob.{k}")));
    keys.extend(error_codes().iter().map(|k| format!("err.{k}")));
    keys.extend(settings_codes().iter().map(|k| format!("settings.err.{k}")));
    let gaps = missing(&dicts, &keys);
    assert!(
        gaps.is_empty(),
        "codes of the core with no text:\n{}",
        gaps.join("\n")
    );
}

/// The string literals of the TypeScript union after `marker` in `src`
/// (up to the first `;`).
fn ts_union(src: &str, marker: &str) -> BTreeSet<String> {
    let at = src
        .find(marker)
        .unwrap_or_else(|| panic!("`{marker}` not found in types.ts"))
        + marker.len();
    let end = at + src[at..].find(';').expect("end of the union");
    literals(&src[at..end])
}

#[test]
fn the_ui_types_list_the_cores_codes() {
    let types = read("app/src/lib/types.ts");
    let warning = body(&types, "export interface RoofWarning");
    let calc = body(&types, "export interface CalcError");
    let problem = body(&types, "export interface Problem ");
    let set = |v: Vec<String>| v.into_iter().collect::<BTreeSet<_>>();
    assert_eq!(
        ts_union(warning, "kind:"),
        set(warning_kinds()),
        "RoofWarning.kind in types.ts"
    );
    assert_eq!(
        ts_union(calc, "kind:"),
        set(calc_error_kinds()),
        "CalcError.kind in types.ts"
    );
    assert_eq!(
        ts_union(problem, "item:"),
        set(problem_items()),
        "Problem.item in types.ts"
    );
    assert_eq!(
        ts_union(problem, "code:"),
        problem_codes(),
        "Problem.code in types.ts"
    );
}

#[test]
fn the_ui_translates_every_error_code_of_the_core() {
    // errors.ts shows "unexpected error" for a code missing from KNOWN,
    // even when its text exists.
    let errors = read("app/src/lib/errors.ts");
    let at = errors
        .find("const KNOWN = new Set([")
        .expect("KNOWN in errors.ts");
    let end = at + errors[at..].find("]);").expect("end of KNOWN");
    let known = literals(&errors[at..end]);
    let gaps: Vec<_> = error_codes()
        .into_iter()
        .filter(|c| !known.contains(c))
        .collect();
    assert!(
        gaps.is_empty(),
        "error codes of the core missing from KNOWN in errors.ts: {gaps:?}"
    );
}
