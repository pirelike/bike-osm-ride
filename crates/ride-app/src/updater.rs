//! Update check against the project's GitHub releases.
//!
//! The repository comes from the crate's `repository` field (`Cargo.toml`); while it is empty
//! the updater does nothing. At start-up the latest release is fetched from the GitHub API
//! (`/repos/<owner>/<repo>/releases/latest`); when its tag is newer than this build the top bar
//! offers it:
//!
//! * **Windows**: the release's `*-windows-x64.exe` asset is downloaded next to the running
//!   executable, the running file is renamed to `*.old` (Windows allows renaming a running
//!   exe), the new one takes its name, the new version is started and this one exits. The
//!   leftover `*.old` is removed on the next start.
//! * **Android**: an app cannot replace itself; the `.apk` asset is opened in the system
//!   browser, which downloads it and hands it to the package installer.
//!
//! Everything that decides (version comparison, asset choice, response parsing) is pure and
//! unit-tested; only [`check`] and [`install`] touch the network and the file system.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

/// `owner/repo` from the crate's `repository` URL, `None` while it is not set.
pub fn repo_slug() -> Option<String> {
    slug_from_url(env!("CARGO_PKG_REPOSITORY"))
}

/// `https://github.com/owner/repo(.git)` → `owner/repo`.
pub fn slug_from_url(url: &str) -> Option<String> {
    let rest = url.trim().strip_prefix("https://github.com/")?;
    let rest = rest.trim_end_matches('/').trim_end_matches(".git");
    let mut parts = rest.split('/');
    let (owner, repo) = (parts.next()?, parts.next()?);
    let ok = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    (ok(owner) && ok(repo) && parts.next().is_none()).then(|| format!("{owner}/{repo}"))
}

/// `v1.2.3` / `1.2.3` / `1.2` → `(1, 2, 3)`; pre-release suffixes (`-beta`) are ignored.
pub fn parse_version(tag: &str) -> Option<(u32, u32, u32)> {
    let core = tag.trim().trim_start_matches(['v', 'V']);
    let core = core.split(['-', '+']).next()?;
    let mut nums = core.split('.').map(|p| p.parse::<u32>());
    let major = nums.next()?.ok()?;
    let minor = nums.next().unwrap_or(Ok(0)).ok()?;
    let patch = nums.next().unwrap_or(Ok(0)).ok()?;
    nums.next().is_none().then_some((major, minor, patch))
}

/// `true` if `latest` is a strictly newer version than `current`.
pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

/// The platforms a release carries a package for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Windows,
    Android,
}

impl Platform {
    pub fn current() -> Option<Platform> {
        if cfg!(target_os = "android") {
            Some(Platform::Android)
        } else if cfg!(windows) {
            Some(Platform::Windows)
        } else {
            None
        }
    }

    fn matches(self, asset_name: &str) -> bool {
        let name = asset_name.to_ascii_lowercase();
        match self {
            Platform::Windows => name.ends_with(".exe") && name.contains("windows"),
            Platform::Android => name.ends_with(".apk"),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct ReleaseJson {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<AssetJson>,
}

#[derive(Debug, Clone, Deserialize)]
struct AssetJson {
    name: String,
    browser_download_url: String,
}

/// A newer release this build can install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateInfo {
    /// The release tag (`v0.2.0`).
    pub version: String,
    /// The release page (shown when the download is not possible).
    pub page_url: String,
    /// The package for this platform.
    pub asset_url: String,
    pub asset_name: String,
}

/// Decides from a `releases/latest` response whether there is something to offer.
pub fn evaluate(json: &str, current: &str, platform: Platform) -> Option<UpdateInfo> {
    let release: ReleaseJson = serde_json::from_str(json).ok()?;
    if release.draft || release.prerelease || !is_newer(&release.tag_name, current) {
        return None;
    }
    let asset = release.assets.iter().find(|a| platform.matches(&a.name))?;
    let trusted = |u: &str| u.starts_with("https://github.com/");
    if !trusted(&asset.browser_download_url) || !trusted(&release.html_url) {
        return None;
    }
    Some(UpdateInfo {
        version: release.tag_name.clone(),
        page_url: release.html_url,
        asset_url: asset.browser_download_url.clone(),
        asset_name: asset.name.clone(),
    })
}

fn client() -> Option<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(concat!("bike-osm-ride/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(20))
        .build()
        .ok()
}

/// Asks GitHub for the latest release; `None` when there is no newer one, no repository is
/// configured or anything fails (an update check never bothers the rider with errors).
pub async fn check() -> Option<UpdateInfo> {
    let slug = repo_slug()?;
    let platform = Platform::current()?;
    let url = format!("https://api.github.com/repos/{slug}/releases/latest");
    let response = client()?
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let body = response.text().await.ok()?;
    evaluate(&body, env!("CARGO_PKG_VERSION"), platform)
}

/// The file a running executable is moved to while its replacement takes its place.
pub fn old_path(exe: &Path) -> PathBuf {
    let mut name = exe.file_name().unwrap_or_default().to_os_string();
    name.push(".old");
    exe.with_file_name(name)
}

/// Removes what a previous update left behind (`*.old` beside the running executable).
pub fn cleanup_previous() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(old_path(&exe));
    }
}

/// Downloads the new executable and swaps it in (Windows). On success the caller starts the
/// new file (returned) and exits.
pub async fn install(update: &UpdateInfo) -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let client = client().ok_or("a HTTP-kliens nem indult el")?;
    let response = client
        .get(&update.asset_url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status().as_u16()));
    }
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    // An exe starts with "MZ"; anything else (an HTML error page) is not installed.
    if bytes.len() < 1024 || !bytes.starts_with(b"MZ") {
        return Err("a letöltött fájl nem futtatható program".into());
    }
    let staged = exe.with_extension("new");
    tokio::fs::write(&staged, &bytes)
        .await
        .map_err(|e| e.to_string())?;
    let old = old_path(&exe);
    let _ = std::fs::remove_file(&old);
    std::fs::rename(&exe, &old).map_err(|e| e.to_string())?;
    if let Err(e) = std::fs::rename(&staged, &exe) {
        // put the running version back so the app still starts next time
        let _ = std::fs::rename(&old, &exe);
        return Err(e.to_string());
    }
    Ok(exe)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RELEASE: &str = r#"{
        "tag_name": "v0.2.0",
        "html_url": "https://github.com/owner/bike-osm-ride/releases/tag/v0.2.0",
        "draft": false,
        "prerelease": false,
        "assets": [
            {"name": "bike-osm-ride-0.2.0-android.apk",
             "browser_download_url": "https://github.com/owner/bike-osm-ride/releases/download/v0.2.0/bike-osm-ride-0.2.0-android.apk"},
            {"name": "bike-osm-ride-0.2.0-windows-x64.exe",
             "browser_download_url": "https://github.com/owner/bike-osm-ride/releases/download/v0.2.0/bike-osm-ride-0.2.0-windows-x64.exe"}
        ]
    }"#;

    #[test]
    fn versions_are_compared_numerically() {
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("0.10"), Some((0, 10, 0)));
        assert_eq!(parse_version("2.0.0-beta.1"), Some((2, 0, 0)));
        assert_eq!(parse_version("nope"), None);
        assert_eq!(parse_version("1.2.3.4"), None);
        assert!(is_newer("v0.10.0", "0.9.9"));
        assert!(!is_newer("v0.1.0", "0.1.0"));
        assert!(!is_newer("garbage", "0.1.0"));
    }

    #[test]
    fn the_repository_url_gives_the_slug() {
        assert_eq!(
            slug_from_url("https://github.com/owner/bike-osm-ride"),
            Some("owner/bike-osm-ride".into())
        );
        assert_eq!(
            slug_from_url("https://github.com/owner/bike-osm-ride.git/"),
            Some("owner/bike-osm-ride".into())
        );
        assert_eq!(slug_from_url(""), None);
        assert_eq!(slug_from_url("https://example.com/owner/repo"), None);
        assert_eq!(slug_from_url("https://github.com/owner/repo/extra"), None);
        assert_eq!(slug_from_url("https://github.com/own er/repo"), None);
    }

    #[test]
    fn a_newer_release_offers_the_package_of_the_platform() {
        let win = evaluate(RELEASE, "0.1.0", Platform::Windows).expect("windows update");
        assert_eq!(win.version, "v0.2.0");
        assert!(win.asset_name.ends_with("windows-x64.exe"));
        let apk = evaluate(RELEASE, "0.1.0", Platform::Android).expect("android update");
        assert!(apk.asset_name.ends_with(".apk"));
        assert_eq!(
            evaluate(RELEASE, "0.2.0", Platform::Windows),
            None,
            "same version"
        );
        assert_eq!(
            evaluate(RELEASE, "0.3.0", Platform::Windows),
            None,
            "older release"
        );
    }

    #[test]
    fn drafts_prereleases_foreign_urls_and_garbage_are_ignored() {
        let pre = RELEASE.replace("\"prerelease\": false", "\"prerelease\": true");
        assert_eq!(evaluate(&pre, "0.1.0", Platform::Windows), None);
        let foreign = RELEASE.replace(
            "https://github.com/owner/bike-osm-ride/releases/download/v0.2.0/bike-osm-ride-0.2.0-windows-x64.exe",
            "https://evil.example/x.exe",
        );
        assert_eq!(evaluate(&foreign, "0.1.0", Platform::Windows), None);
        assert_eq!(evaluate("{}", "0.1.0", Platform::Windows), None);
        assert_eq!(evaluate("not json", "0.1.0", Platform::Android), None);
    }

    #[test]
    fn the_old_file_sits_beside_the_executable() {
        let exe = Path::new("C:/apps/bike-osm-ride.exe");
        assert_eq!(old_path(exe), Path::new("C:/apps/bike-osm-ride.exe.old"));
    }
}
