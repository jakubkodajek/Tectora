//! The library card of a job with a stored result (the offer's figures):
//! it carries the stored area and says when the current roof differs from
//! it, so the library never shows a figure the customer was not offered as
//! if it were the offer's (review UX-008).

use tectora_core::store::Store;
use tectora_core::{examples, snapshot};

fn temp_store(name: &str) -> Store {
    let dir = std::env::temp_dir().join(format!("tectora-offer-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    // Figures are checked: waited for as long as a loaded runner needs,
    // not the app's half second (audit 2026-11-22 TEST-603).
    Store::new(dir)
        .unwrap()
        .with_summary_budget(std::time::Duration::from_secs(60))
}

#[test]
fn an_offer_card_has_the_stored_area_and_says_when_the_roof_changed() {
    let store = temp_store("differs");
    let mut p = examples::find("gable").unwrap().project();
    p.result_snapshot = Some(snapshot::take(&p));
    let stored = p.result_snapshot.as_ref().unwrap().roof_area;
    let id = store.save(None, &p).unwrap();

    let s = store.summarize(&id).unwrap();
    assert!(!s.figures_pending, "{s:?}");
    assert_eq!(s.stored_area, Some(stored));
    assert!(!s.result_differs, "unchanged roof: {s:?}");

    // Steeper roof after the offer: the area changes, the stored one stays.
    for e in p.parts[0].edges.iter_mut().filter(|e| !e.is_gable()) {
        e.pitch_deg = 45.0;
    }
    store.save(Some(id.clone()), &p).unwrap();
    let s = store.summarize(&id).unwrap();
    assert!(!s.figures_pending, "{s:?}");
    assert_eq!(s.stored_area, Some(stored));
    assert!(s.result_differs, "changed roof: {s:?}");
    assert!((s.roof_area - stored).abs() > 1.0, "{s:?}");
    let _ = std::fs::remove_dir_all(&store.dir);
}

#[test]
fn a_draft_card_has_no_stored_area() {
    let store = temp_store("draft");
    let id = store
        .save(None, &examples::find("hip").unwrap().project())
        .unwrap();
    let s = store.summarize(&id).unwrap();
    assert_eq!(s.stored_area, None);
    assert!(!s.result_differs);
    let _ = std::fs::remove_dir_all(&store.dir);
}
