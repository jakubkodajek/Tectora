//! Updates of the app itself. The job library asks whether the update
//! server offers a newer version (`update_check`) and installs it on the
//! user's go (`update_install`), which ends the app and starts the new
//! version. The library offers it only while no job is open in the editor,
//! so nothing unsaved is lost. Each update is signed with the release key,
//! and the download is checked against its public half (`plugins.updater`
//! in tauri.conf.json) before anything is installed (docs/releasing.md).

use std::time::Duration;

use serde::Serialize;
use tauri::async_runtime::Mutex;
use tauri::ipc::Channel;
use tauri::{AppHandle, State};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::error::AppError;

/// The longest a check may take: an office without internet must not keep
/// the library waiting for its answer.
const CHECK_TIMEOUT: Duration = Duration::from_secs(15);
/// The longest the download of an update may take, on a slow connection.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// The most of a release's notes the UI is given.
const MAX_NOTES_CHARS: usize = 1_000;
/// How often the download reports its progress when the server does not
/// say how large the update is.
const UNSIZED_PROGRESS_STEP: u64 = 512 * 1024;

/// A newer version the update server offers.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AvailableUpdate {
    pub version: String,
    /// What the release says changed, cut after `MAX_NOTES_CHARS`; empty
    /// when it says nothing.
    pub notes: String,
}

/// How much of an update has been downloaded.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UpdateProgress {
    pub downloaded: u64,
    /// The update's size, when the server says it.
    pub total: Option<u64>,
}

/// What a check that got an answer found.
enum Checked {
    Newer(Box<Update>),
    Current,
}

/// The first answer the update server gave in this run, kept so that what
/// is installed is what the user was offered. A check that fails is not
/// kept: the next one asks again, as the network may come up later.
#[derive(Default)]
pub struct Updates(Mutex<Option<Checked>>);

/// The newer version the update server offers, if any. The server is asked
/// until it answers once; later checks give that answer. Debug builds,
/// which are run from the source, never update.
#[tauri::command]
pub async fn update_check(
    app: AppHandle,
    updates: State<'_, Updates>,
) -> Result<Option<AvailableUpdate>, AppError> {
    let mut checked = updates.0.lock().await;
    if checked.is_none() {
        let answer = check(&app)
            .await
            .inspect_err(|e| eprintln!("tectora: checking for an update failed: {e}"))?;
        *checked = Some(answer);
    }
    Ok(match checked.as_ref() {
        Some(Checked::Newer(update)) => Some(available(update)),
        Some(Checked::Current) | None => None,
    })
}

async fn check(app: &AppHandle) -> tauri_plugin_updater::Result<Checked> {
    if cfg!(debug_assertions) {
        return Ok(Checked::Current);
    }
    let update = app
        .updater_builder()
        .timeout(CHECK_TIMEOUT)
        .build()?
        .check()
        .await?;
    Ok(update.map_or(Checked::Current, |update| Checked::Newer(Box::new(update))))
}

/// What the UI is told of `update`.
fn available(update: &Update) -> AvailableUpdate {
    let notes = update.body.as_deref().unwrap_or_default().trim();
    AvailableUpdate {
        version: update.version.clone(),
        notes: notes.chars().take(MAX_NOTES_CHARS).collect(),
    }
}

/// Downloads the update the check found, verifies its signature and
/// installs it, telling `progress` how the download goes. On Windows the
/// installer closes the app and starts the new version; elsewhere the app
/// starts again once the new version is in place.
#[tauri::command]
pub async fn update_install(
    app: AppHandle,
    updates: State<'_, Updates>,
    progress: Channel<UpdateProgress>,
) -> Result<(), AppError> {
    // Held to the end, so a second install waits for this one.
    let checked = updates.0.lock().await;
    let Some(Checked::Newer(update)) = checked.as_ref() else {
        return Err(AppError::new("no_update"));
    };
    let mut update = Update::clone(update);
    update.timeout = Some(DOWNLOAD_TIMEOUT);
    let mut downloaded = 0;
    let bytes = update
        .download(
            |chunk, total| {
                let before = downloaded;
                downloaded += chunk as u64;
                if progress_step_crossed(before, downloaded, total) {
                    // A window that went away needs no progress.
                    let _ = progress.send(UpdateProgress { downloaded, total });
                }
            },
            || (),
        )
        .await
        .inspect_err(|e| {
            eprintln!(
                "tectora: downloading version {} failed: {e}",
                update.version
            )
        })?;
    // Installing may wait for a password dialog on macOS, which needs the
    // main thread free.
    tauri::async_runtime::spawn_blocking(move || {
        eprintln!(
            "tectora: installing version {} over {}",
            update.version, update.current_version
        );
        // On Windows the installer takes over and the app ends here.
        update.install(&bytes).inspect_err(|e| {
            eprintln!("tectora: installing version {} failed: {e}", update.version)
        })?;
        app.restart()
    })
    .await
    .map_err(|e| AppError::new("internal").detail(e.to_string()))?
}

/// Whether the download reports its progress on going from `before` to
/// `after` bytes: on each further hundredth of `total` and at its end, or
/// each `UNSIZED_PROGRESS_STEP` when the size is not known.
fn progress_step_crossed(before: u64, after: u64, total: Option<u64>) -> bool {
    let step = total.map_or(UNSIZED_PROGRESS_STEP, |total| (total / 100).max(1));
    before / step != after / step || total == Some(after)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_download_reports_each_hundredth_once() {
        let total = Some(10_000);
        assert!(!progress_step_crossed(0, 99, total));
        assert!(progress_step_crossed(99, 100, total));
        assert!(!progress_step_crossed(100, 150, total));
        // A large chunk crossing several steps reports once.
        assert!(progress_step_crossed(150, 1_000, total));
        assert!(progress_step_crossed(9_999, 10_000, total));
    }

    #[test]
    fn the_end_of_the_download_is_reported() {
        // The last bytes cross no hundredth, yet the UI learns the download is complete.
        assert!(progress_step_crossed(10_000, 10_050, Some(10_050)));
        assert!(!progress_step_crossed(10_000, 10_040, Some(10_050)));
    }

    #[test]
    fn a_download_of_unknown_size_reports_every_half_megabyte() {
        assert!(!progress_step_crossed(0, UNSIZED_PROGRESS_STEP - 1, None));
        assert!(progress_step_crossed(
            UNSIZED_PROGRESS_STEP - 1,
            UNSIZED_PROGRESS_STEP,
            None
        ));
    }

    #[test]
    fn a_tiny_update_reports_every_byte() {
        assert!(progress_step_crossed(0, 1, Some(50)));
        assert!(progress_step_crossed(1, 2, Some(50)));
    }

    fn config() -> serde_json::Value {
        serde_json::from_str(include_str!("../tauri.conf.json")).unwrap()
    }

    #[test]
    fn the_update_server_is_asked_over_https_for_a_signed_update() {
        let config = config();
        let updater = &config["plugins"]["updater"];
        let endpoints = updater["endpoints"].as_array().unwrap();
        assert_eq!(endpoints.len(), 1, "release.yml publishes to one place");
        let endpoint = endpoints[0].as_str().unwrap();
        // release.yml reads the repository it publishes to from this address.
        let repo = endpoint
            .strip_prefix("https://github.com/")
            .and_then(|rest| rest.strip_suffix("/releases/latest/download/latest.json"))
            .unwrap_or_else(|| panic!("not a GitHub latest.json address: {endpoint}"));
        assert_eq!(repo.split('/').count(), 2, "{endpoint}");
        assert!(updater["dangerousInsecureTransportProtocol"].is_null());
        assert_eq!(updater["requireSignedVersion"], true);
        assert!(updater["pubkey"].is_string());
        // Builds made without the release key stay possible: only a release asks for updater bundles.
        assert!(config["bundle"]["createUpdaterArtifacts"].is_null());
        let release: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.release.conf.json")).unwrap();
        assert_eq!(release["bundle"]["createUpdaterArtifacts"], true);
    }

    /// The installed app compares the version in tauri.conf.json; the
    /// crates and the UI package carry the same one (`tectora --version`,
    /// the stored results' engine version).
    #[test]
    fn every_manifest_declares_the_same_version() {
        let app = config()["version"].as_str().unwrap().to_string();
        assert_eq!(
            app,
            env!("CARGO_PKG_VERSION"),
            "Cargo.toml [workspace.package]"
        );
        let package: serde_json::Value =
            serde_json::from_str(include_str!("../../package.json")).unwrap();
        assert_eq!(package["version"], app.as_str(), "app/package.json");
    }
}
