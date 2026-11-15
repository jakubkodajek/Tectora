//! Searching and sorting the job library (the roofs made so far), and
//! starting a new job from one of them.
//!
//! The app filters the summaries it already holds in the UI; the MCP server
//! and the CLI use [`search`] so an AI client can do the same: free text,
//! status, last change by an AI, date range, sort order and a limit.

use serde::Deserialize;

use crate::model::{JobInfo, JobStatus, Project};
use crate::store::{ProjectSummary, USER_ACTOR};

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
    let words: Vec<String> = fold(&q.text)
        .split_whitespace()
        .map(str::to_string)
        .collect();
    let mut out: Vec<ProjectSummary> = list
        .into_iter()
        .filter(|p| {
            q.status.is_none_or(|s| p.job.status == s)
                && (!q.by_ai
                    || p.last_activity
                        .as_ref()
                        .is_some_and(|a| a.actor != USER_ACTOR))
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

/// Lower case without the Czech, Slovak and German accents, for search and
/// sorting ("Říčany" matches "ricany").
pub fn fold(s: &str) -> String {
    s.to_lowercase().chars().map(fold_char).collect()
}

pub(crate) fn fold_char(c: char) -> char {
    match c {
        'á' | 'ä' | 'à' | 'â' => 'a',
        'č' | 'ç' => 'c',
        'ď' => 'd',
        'é' | 'ě' | 'è' | 'ê' | 'ë' => 'e',
        'í' | 'î' => 'i',
        'ĺ' | 'ľ' => 'l',
        'ň' => 'n',
        'ó' | 'ô' | 'ö' => 'o',
        'ŕ' | 'ř' => 'r',
        'š' | 'ß' => 's',
        'ť' => 't',
        'ú' | 'ů' | 'ü' => 'u',
        'ý' => 'y',
        'ž' => 'z',
        c => c,
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
            pitch_range: None,
            dormers: 0,
            files: 0,
            has_result: false,
            figures_pending: false,
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
        });
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
