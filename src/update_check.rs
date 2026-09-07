//! Read-only GitHub version detection. No installer, filesystem or database access.
//! Packaging can set DPT_INSTALLATION_KIND at build time. Asset/architecture
//! matching and installation belong in a future packaging layer, not this checker.
use std::{sync::mpsc, time::Duration};

use semver::Version;
use serde::Deserialize;

pub const SOURCE_URL: &str = env!("CARGO_PKG_REPOSITORY");
pub const RELEASES_URL: &str = "https://github.com/ArnieSkyNet/DirectPaymentTimesheets/releases";
const API_URL: &str = "https://api.github.com/repos/ArnieSkyNet/DirectPaymentTimesheets";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallationKind {
    Source,
    Deb,
    AppImage,
    Windows,
    MacOs,
    Unknown,
}

impl InstallationKind {
    fn from_marker(marker: Option<&str>) -> Self {
        match marker {
            None | Some("source") => Self::Source,
            Some("deb") => Self::Deb,
            Some("appimage") => Self::AppImage,
            Some("windows") => Self::Windows,
            Some("macos") => Self::MacOs,
            _ => Self::Unknown,
        }
    }

    pub fn current() -> Self {
        Self::from_marker(option_env!("DPT_INSTALLATION_KIND"))
    }

    pub fn guidance(self) -> &'static str {
        match self {
            Self::Source => "Source build: review the release/source and rebuild manually. This check never overwrites your checkout.",
            Self::Deb => "Debian package: review GitHub release assets. Package/architecture matching and installation are not yet implemented.",
            Self::AppImage => "AppImage: review GitHub release assets. Package/architecture matching and installation are not yet implemented.",
            Self::Windows => "Windows package: review GitHub release assets. Package/architecture matching and installation are not yet implemented.",
            Self::MacOs => "macOS package: review GitHub release assets. Package/architecture matching and installation are not yet implemented.",
            Self::Unknown => "Unrecognised installation type: review the release/source manually.",
        }
    }
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
}

#[derive(Deserialize)]
struct Tag {
    name: String,
}

fn parsed_version(tag: &str) -> Option<Version> {
    Version::parse(tag.strip_prefix('v').unwrap_or(tag)).ok()
}

fn newest_version<'a>(tags: impl Iterator<Item = &'a str>) -> Option<Version> {
    tags.filter_map(parsed_version)
        .max_by(|a, b| a.cmp_precedence(b))
}

fn check() -> Result<String, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .build()
        .into();
    // Explicitly bounded metadata reads, including prereleases and un-released tags.
    // Never claim that a bounded list is an exhaustive release history.
    let releases: Vec<Release> = agent
        .get(&format!("{API_URL}/releases?per_page=100"))
        .header("User-Agent", "DirectPaymentTimesheets-update-check")
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("GitHub release check failed: {e}"))?
        .body_mut()
        .read_json()
        .map_err(|e| format!("Invalid GitHub release response: {e}"))?;
    let tags: Vec<Tag> = agent
        .get(&format!("{API_URL}/tags?per_page=100"))
        .header("User-Agent", "DirectPaymentTimesheets-update-check")
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("GitHub tag check failed: {e}"))?
        .body_mut()
        .read_json()
        .map_err(|e| format!("Invalid GitHub tag response: {e}"))?;
    let newest = newest_version(
        releases
            .iter()
            .filter(|r| !r.draft)
            .map(|r| r.tag_name.as_str())
            .chain(tags.iter().map(|t| t.name.as_str())),
    );
    Ok(version_status(env!("CARGO_PKG_VERSION"), newest))
}

fn version_status(current: &str, newest: Option<Version>) -> String {
    let Some(newest) = newest else {
        return "No comparable semantic-version releases/tags found. Review GitHub manually."
            .into();
    };
    let Ok(current) = Version::parse(current) else {
        return "Unable to compare this build version. Review GitHub manually.".into();
    };
    let comparison = if newest.cmp_precedence(&current).is_gt() {
        "A newer release/tag is available"
    } else {
        "No newer release/tag found"
    };
    format!("{comparison}. Highest version checked: {newest}. Checked up to 100 releases and 100 tags, including prereleases; tags may not have packaged assets.")
}

#[derive(Default)]
pub struct UpdateCheck {
    pending: Option<mpsc::Receiver<Result<String, String>>>,
    pub status: Option<String>,
}

impl UpdateCheck {
    pub fn running(&self) -> bool {
        self.pending.is_some()
    }

    pub fn start(&mut self, ctx: egui::Context) {
        if self.running() {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.pending = Some(receiver);
        self.status = Some("Checking GitHub releases and tags…".into());
        std::thread::spawn(move || {
            let _ = sender.send(check());
            ctx.request_repaint();
        });
    }

    pub fn poll(&mut self) {
        let Some(receiver) = &self.pending else {
            return;
        };
        match receiver.try_recv() {
            Ok(result) => {
                self.status = Some(result.unwrap_or_else(|error| {
                    format!("{error}. You can retry or review GitHub manually.")
                }));
                self.pending = None;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.status = Some("Update check stopped unexpectedly. Please retry.".into());
                self.pending = None;
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_semver_not_lexical_order_and_ignores_nonversion_tags() {
        let newest = newest_version(["v0.0.9", "0.0.11", "v0.0.12-rc.1", "nightly"].into_iter());
        assert_eq!(newest, Some(Version::parse("0.0.12-rc.1").unwrap()));
        assert!(version_status("0.0.11", newest).starts_with("A newer"));
        assert_eq!(newest_version(["nightly"].into_iter()), None);
    }

    #[test]
    fn equal_older_and_build_metadata_do_not_offer_updates() {
        for tag in ["v0.0.10", "v0.0.11", "v0.0.11+package.2"] {
            assert!(version_status("0.0.11", parsed_version(tag)).starts_with("No newer"));
        }
        assert!(version_status("0.0.12-rc.1", parsed_version("v0.0.12")).starts_with("A newer"));
        assert!(version_status("0.0.11", None).starts_with("No comparable"));
    }

    #[test]
    fn installation_kind_is_explicit_not_guessed_from_os() {
        assert_eq!(
            InstallationKind::from_marker(None),
            InstallationKind::Source
        );
        for (marker, expected) in [
            ("deb", InstallationKind::Deb),
            ("appimage", InstallationKind::AppImage),
            ("windows", InstallationKind::Windows),
            ("macos", InstallationKind::MacOs),
            ("unexpected", InstallationKind::Unknown),
        ] {
            assert_eq!(InstallationKind::from_marker(Some(marker)), expected);
        }
    }
}
