//! Update check against the project's GitHub releases.
//!
//! The repository comes from the crate's `repository` field (`Cargo.toml`); while it is empty
//! the updater does nothing. At start-up the latest release is fetched from the GitHub API
//! (`/repos/<owner>/<repo>/releases/latest`); when its tag is newer than this build the top bar
//! offers it.
//!
//! ## Asset contract
//!
//! A release carries one package per platform, named
//! `bike-osm-ride-<version>-<platform>.<ext>` (see `README.md` and the header of
//! `.github/workflows/release.yml`):
//!
//! | platform | asset | install |
//! |---|---|---|
//! | Windows | `…-windows-x64.exe` | download, swap itself, restart |
//! | Android | `…-android.apk` | open the APK, the system installer takes over |
//! | Linux (AppImage) | `…-linux-x86_64.AppImage` | download, swap the `.AppImage`, restart |
//! | Linux (.deb) | `…-linux-amd64.deb` | open the release page |
//! | macOS | `…-macos-universal.dmg` | open the release page |
//!
//! Only the AppImage can replace itself on Linux: it is a single executable file, while a `.deb`
//! installation lives in `/usr` and belongs to the package manager. macOS cannot replace a
//! quarantined, ad-hoc signed bundle either, so both open the release page in the browser
//! instead (the same idea as the Android branch) and a toast tells the rider what to download.
//!
//! Everything that decides (version comparison, asset choice, install mode, magic bytes) is
//! pure and unit-tested; only [`check`], [`install`] and [`open_release_page`] touch the network,
//! the browser and the file system.

use std::path::{Path, PathBuf};
use std::time::Duration;

use ride_i18n::t;
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

/// The magic bytes that identify an executable of a platform we can replace ourselves with.
const ELF_MAGIC: &[u8] = b"\x7fELF";
/// Anything smaller than this is an error page or a truncated download, not a program.
const MIN_PROGRAM_BYTES: usize = 1024;

/// The platforms a release carries a package for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Windows,
    Android,
    Linux,
    MacOs,
}

impl Platform {
    pub fn current() -> Option<Platform> {
        if cfg!(target_os = "android") {
            Some(Platform::Android)
        } else if cfg!(windows) {
            Some(Platform::Windows)
        } else if cfg!(target_os = "macos") {
            Some(Platform::MacOs)
        } else if cfg!(target_os = "linux") {
            Some(Platform::Linux)
        } else {
            None
        }
    }

    /// Is `asset_name` the package of this platform?
    fn matches(self, asset_name: &str, version: &str) -> bool {
        let suffix = match self {
            Platform::Windows => "windows-x64.exe",
            Platform::Android => "android.apk",
            Platform::Linux => "linux-x86_64.AppImage",
            Platform::MacOs => "macos-universal.dmg",
        };
        asset_name == format!("bike-osm-ride-{}-{suffix}", version.trim_start_matches('v'))
    }

    /// Magic bytes of a program this platform can swap itself with, `None` when it never does.
    pub fn executable_magic(self) -> Option<&'static [u8]> {
        match self {
            Platform::Windows => Some(b"MZ"),
            Platform::Linux => Some(ELF_MAGIC),
            // The .apk and the .dmg are installed by the system / the user, not by us.
            Platform::Android | Platform::MacOs => None,
        }
    }
}

/// What the "Frissítés" button does for the package of `platform`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallMode {
    /// Download, verify, swap the running file and start the new version.
    ReplaceSelf,
    /// The package cannot replace itself: open the release page and explain what to install.
    OpenReleasePage,
}

/// The install mode of `platform`. `appimage` is the `APPIMAGE` environment variable, which the
/// AppImage runtime sets to the path of the `.AppImage` file it has mounted.
pub fn install_mode(platform: Platform, appimage: Option<&str>) -> InstallMode {
    match platform {
        Platform::Windows => InstallMode::ReplaceSelf,
        Platform::Linux => match appimage.map(str::trim) {
            Some(path) if !path.is_empty() => InstallMode::ReplaceSelf,
            _ => InstallMode::OpenReleasePage,
        },
        Platform::Android | Platform::MacOs => InstallMode::OpenReleasePage,
    }
}

/// [`install_mode`] with the environment of this process.
pub fn current_install_mode(platform: Platform) -> InstallMode {
    let appimage = std::env::var("APPIMAGE").ok();
    install_mode(platform, appimage.as_deref())
}

/// The file a self-replacing update overwrites: the running executable, or — inside an AppImage
/// — the `.AppImage` file the runtime mounted, which is the one the user downloaded.
pub fn install_target(platform: Platform, exe: &Path, appimage: Option<&Path>) -> PathBuf {
    match platform {
        Platform::Linux => appimage
            .map(Path::to_path_buf)
            .unwrap_or_else(|| exe.to_path_buf()),
        _ => exe.to_path_buf(),
    }
}

/// Does `bytes` start like a program this platform can replace itself with?
pub fn looks_like_program(platform: Platform, bytes: &[u8]) -> bool {
    match platform.executable_magic() {
        Some(magic) => bytes.len() >= MIN_PROGRAM_BYTES && bytes.starts_with(magic),
        None => false,
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
    pub checksum_url: String,
}

/// Decides from a `releases/latest` response whether there is something to offer.
pub fn evaluate(json: &str, current: &str, platform: Platform) -> Option<UpdateInfo> {
    let release: ReleaseJson = serde_json::from_str(json).ok()?;
    if release.draft || release.prerelease || !is_newer(&release.tag_name, current) {
        return None;
    }
    let asset = release
        .assets
        .iter()
        .find(|a| platform.matches(&a.name, &release.tag_name))?;
    let prefix = format!("https://github.com/{}/releases/", repo_slug()?);
    let trusted = |u: &str| u.starts_with(&prefix);
    let checksum = release.assets.iter().find(|a| a.name == "SHA256SUMS")?;
    let download_prefix = format!("{prefix}download/{}/", release.tag_name);
    if !checksum.browser_download_url.starts_with(&download_prefix)
        || !asset.browser_download_url.starts_with(&download_prefix)
    {
        return None;
    }
    if !trusted(&asset.browser_download_url) || !trusted(&release.html_url) {
        return None;
    }
    Some(UpdateInfo {
        version: release.tag_name.clone(),
        page_url: release.html_url,
        asset_url: asset.browser_download_url.clone(),
        asset_name: asset.name.clone(),
        checksum_url: checksum.browser_download_url.clone(),
    })
}

fn client() -> Option<reqwest::Client> {
    ride_route::http::client_builder()
        .user_agent(concat!("bike-osm-ride/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(600))
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
        .timeout(Duration::from_secs(20))
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

/// Where the downloaded bytes are staged before the swap (same directory, so the rename is one
/// filesystem operation).
fn staged_path(target: &Path) -> PathBuf {
    target.with_extension("new")
}

/// Removes what a previous update left behind (`*.old` beside the running executable and beside
/// the AppImage).
pub fn cleanup_previous() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(old_path(&exe));
        let _ = std::fs::remove_file(staged_path(&exe));
    }
    // Inside an AppImage the running executable is in the read-only mount; the file the update
    // replaced is the `.AppImage` itself.
    if let Some(appimage) = std::env::var_os("APPIMAGE").map(PathBuf::from) {
        let _ = std::fs::remove_file(old_path(&appimage));
        let _ = std::fs::remove_file(staged_path(&appimage));
    }
}

/// Downloads the new program and swaps it in (Windows, or Linux inside an AppImage). On success
/// the caller starts the returned file and exits.
pub async fn install(update: &UpdateInfo, platform: Platform) -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(file_error)?;
    let appimage = std::env::var_os("APPIMAGE").map(PathBuf::from);
    let target = install_target(platform, &exe, appimage.as_deref());

    let client = client().ok_or(t("app.updater.a_http_kliens_nem_indult_el"))?;
    let response = client
        .get(&update.asset_url)
        .send()
        .await
        .map_err(network_error)?;
    if !response.status().is_success() {
        return Err(t("app.updater.network_failed").into());
    }
    let bytes = read_limited(response, 256 * 1024 * 1024).await?;
    let checksums = client
        .get(&update.checksum_url)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(network_error)?;
    let checksums = read_limited(checksums, 64 * 1024).await?;
    if !verify_checksum(&bytes, &checksums, &update.asset_name) {
        return Err(t("app.updater.checksum_failed").into());
    }
    // An exe starts with "MZ", an AppImage with "\x7fELF"; anything else (an HTML error page, a
    // link to a login wall) is not installed.
    if !looks_like_program(platform, &bytes) {
        return Err(t("app.updater.a_letoltott_fajl_nem_futtathato_program").into());
    }

    let staged = staged_path(&target);
    tokio::fs::write(&staged, &bytes)
        .await
        .map_err(file_error)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // The download is written with the umask, and the AppImage must stay executable.
        tokio::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755))
            .await
            .map_err(file_error)?;
    }
    let swap_target = target.clone();
    tokio::task::spawn_blocking(move || replace_program(&swap_target, &staged))
        .await
        .map_err(|_| t("app.updater.file_failed").to_string())?
        .map_err(file_error)?;
    Ok(target)
}

/// Publish a fully staged file without first removing the launchable target.
fn replace_program(target: &Path, staged: &Path) -> std::io::Result<()> {
    let old = old_path(target);
    let _ = std::fs::remove_file(&old);
    // Flush image bytes before changing directory entries.
    std::fs::OpenOptions::new()
        .write(true)
        .open(staged)?
        .sync_all()?;
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::ReplaceFileW;
        let wide = |p: &Path| {
            p.as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect::<Vec<_>>()
        };
        let (t, s, b) = (wide(target), wide(staged), wide(&old));
        // SAFETY: nul-terminated path buffers outlive the call; unused pointers are null.
        if unsafe {
            ReplaceFileW(
                t.as_ptr(),
                s.as_ptr(),
                b.as_ptr(),
                0,
                std::ptr::null(),
                std::ptr::null(),
            )
        } == 0
        {
            let error = std::io::Error::last_os_error();
            if !target.exists() && old.exists() {
                let _ = std::fs::rename(&old, target);
            }
            return Err(error);
        }
    }
    #[cfg(not(windows))]
    {
        std::fs::hard_link(target, &old)?;
        std::fs::rename(staged, target)?;
        if let Some(parent) = target.parent() {
            std::fs::File::open(parent)?.sync_all()?;
        }
    }
    Ok(())
}

fn network_error(error: reqwest::Error) -> String {
    eprintln!("[ride-app] update network: {error}");
    t("app.updater.network_failed").into()
}

fn file_error(error: std::io::Error) -> String {
    eprintln!("[ride-app] update file: {error}");
    t("app.updater.file_failed").into()
}

async fn read_limited(mut response: reqwest::Response, limit: usize) -> Result<Vec<u8>, String> {
    if !response.status().is_success()
        || response.content_length().is_some_and(|n| n > limit as u64)
    {
        return Err(t("app.updater.network_failed").into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(network_error)? {
        if chunk.len() > limit.saturating_sub(bytes.len()) {
            return Err(t("app.updater.network_failed").into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

/// SHA256SUMS comes from the same repository and release as the exact platform asset.
/// This verifies integrity against the published digest; it is not a publisher signature.
fn verify_checksum(bytes: &[u8], manifest: &[u8], name: &str) -> bool {
    use sha2::{Digest, Sha256};
    let Ok(manifest) = std::str::from_utf8(manifest) else {
        return false;
    };
    let mut hashes = manifest.lines().filter_map(|line| {
        let (hash, file) = line.split_once("  ")?;
        (file == name).then_some(hash)
    });
    let Some(hash) = hashes.next() else {
        return false;
    };
    hashes.next().is_none()
        && hash.len() == 64
        && hash.eq_ignore_ascii_case(&format!("{:x}", Sha256::digest(bytes)))
}

/// Opens the release page in the system browser. Only the platforms whose package the user has
/// to install by hand have a browser helper here; the others never ask for one.
pub fn open_release_page(update: &UpdateInfo) -> bool {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        webbrowser::open(&update.page_url).is_ok()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = update;
        false
    }
}

/// What the rider has to do when the package cannot be installed from inside the app.
pub fn manual_install_hint(platform: Platform) -> &'static str {
    match platform {
        Platform::Linux => t("app.updater.nyisd_meg_a_kiadas_oldalat_toltsd_le_a_deb_csomagot_"),
        Platform::MacOs => t("app.updater.nyisd_meg_a_kiadas_oldalat_toltsd_le_a_dmg_t_es_huzd"),
        Platform::Android => t("app.updater.toltsd_le_es_telepitsd_az_uj_apk_t"),
        Platform::Windows => t("app.updater.toltsd_le_az_uj_exe_t_a_kiadas_oldalarol"),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn program_replacement_keeps_a_backup_and_a_failed_stage_keeps_the_target() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("app.exe");
        let staged = super::staged_path(&target);
        std::fs::write(&target, b"old").unwrap();
        std::fs::write(&staged, b"new").unwrap();
        super::replace_program(&target, &staged).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
        assert_eq!(std::fs::read(super::old_path(&target)).unwrap(), b"old");
        assert!(super::replace_program(&target, &staged).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
    }

    use super::*;

    const RELEASE: &str = r#"{
        "tag_name": "v0.2.0",
        "html_url": "https://github.com/pirelike/bike-osm-ride/releases/tag/v0.2.0",
        "draft": false,
        "prerelease": false,
        "assets": [
            {"name": "SHA256SUMS", "browser_download_url": "https://github.com/pirelike/bike-osm-ride/releases/download/v0.2.0/SHA256SUMS"},
            {"name": "bike-osm-ride-0.2.0-android.apk",
             "browser_download_url": "https://github.com/pirelike/bike-osm-ride/releases/download/v0.2.0/bike-osm-ride-0.2.0-android.apk"},
            {"name": "bike-osm-ride-0.2.0-windows-x64.exe",
             "browser_download_url": "https://github.com/pirelike/bike-osm-ride/releases/download/v0.2.0/bike-osm-ride-0.2.0-windows-x64.exe"},
            {"name": "bike-osm-ride-0.2.0-linux-x86_64.AppImage",
             "browser_download_url": "https://github.com/pirelike/bike-osm-ride/releases/download/v0.2.0/bike-osm-ride-0.2.0-linux-x86_64.AppImage"},
            {"name": "bike-osm-ride-0.2.0-linux-amd64.deb",
             "browser_download_url": "https://github.com/pirelike/bike-osm-ride/releases/download/v0.2.0/bike-osm-ride-0.2.0-linux-amd64.deb"},
            {"name": "bike-osm-ride-0.2.0-macos-universal.dmg",
             "browser_download_url": "https://github.com/pirelike/bike-osm-ride/releases/download/v0.2.0/bike-osm-ride-0.2.0-macos-universal.dmg"}
        ]
    }"#;

    #[test]
    fn downloaded_bytes_must_match_the_unique_published_digest() {
        use sha2::{Digest, Sha256};
        let manifest = format!("{:x}  app.exe\n", Sha256::digest(b"program"));
        assert!(verify_checksum(b"program", manifest.as_bytes(), "app.exe"));
        assert!(!verify_checksum(b"changed", manifest.as_bytes(), "app.exe"));
        assert!(!verify_checksum(
            b"program",
            manifest.as_bytes(),
            "other.exe"
        ));
        assert!(!verify_checksum(
            b"program",
            manifest.repeat(2).as_bytes(),
            "app.exe"
        ));
        assert!(evaluate(
            &RELEASE.replace("SHA256SUMS", "missing"),
            "0.1.0",
            Platform::Windows
        )
        .is_none());
        assert!(evaluate(
            &RELEASE.replace("windows-x64.exe", "windows-arm64.exe"),
            "0.1.0",
            Platform::Windows
        )
        .is_none());
    }

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
    fn a_newer_release_offers_the_package_of_each_platform() {
        let win = evaluate(RELEASE, "0.1.0", Platform::Windows).expect("windows update");
        assert_eq!(win.version, "v0.2.0");
        assert!(win.asset_name.ends_with("windows-x64.exe"));

        let apk = evaluate(RELEASE, "0.1.0", Platform::Android).expect("android update");
        assert!(apk.asset_name.ends_with(".apk"));

        let appimage = evaluate(RELEASE, "0.1.0", Platform::Linux).expect("linux update");
        assert!(
            appimage.asset_name.ends_with("linux-x86_64.AppImage"),
            "the AppImage is the Linux update, not the .deb: {}",
            appimage.asset_name
        );

        let dmg = evaluate(RELEASE, "0.1.0", Platform::MacOs).expect("macos update");
        assert!(dmg.asset_name.ends_with("macos-universal.dmg"));

        assert_eq!(
            evaluate(RELEASE, "0.2.0", Platform::Windows),
            None,
            "same version"
        );
        assert_eq!(
            evaluate(RELEASE, "0.3.0", Platform::Linux),
            None,
            "older release"
        );
    }

    #[test]
    fn drafts_prereleases_foreign_urls_and_garbage_are_ignored() {
        let pre = RELEASE.replace("\"prerelease\": false", "\"prerelease\": true");
        assert_eq!(evaluate(&pre, "0.1.0", Platform::Windows), None);
        let foreign = RELEASE.replace(
            "https://github.com/pirelike/bike-osm-ride/releases/download/v0.2.0/bike-osm-ride-0.2.0-windows-x64.exe",
            "https://evil.example/x.exe",
        );
        assert_eq!(evaluate(&foreign, "0.1.0", Platform::Windows), None);
        assert_eq!(evaluate("{}", "0.1.0", Platform::Windows), None);
        assert_eq!(evaluate("not json", "0.1.0", Platform::Android), None);
    }

    #[test]
    fn only_the_appimage_and_the_exe_are_installed_in_place() {
        // Windows always replaces itself, whatever the environment says.
        assert_eq!(
            install_mode(Platform::Windows, None),
            InstallMode::ReplaceSelf
        );
        assert_eq!(
            install_mode(Platform::Windows, Some("/home/u/bike.AppImage")),
            InstallMode::ReplaceSelf
        );
        // Linux: only inside an AppImage (the runtime sets APPIMAGE).
        assert_eq!(
            install_mode(Platform::Linux, Some("/home/u/Applications/bike.AppImage")),
            InstallMode::ReplaceSelf
        );
        for none in [None, Some(""), Some("   ")] {
            assert_eq!(
                install_mode(Platform::Linux, none),
                InstallMode::OpenReleasePage,
                "a .deb installation cannot replace itself: {none:?}"
            );
        }
        // Never in place: the system installs these.
        assert_eq!(
            install_mode(Platform::Android, None),
            InstallMode::OpenReleasePage
        );
        assert_eq!(
            install_mode(Platform::MacOs, Some("/Applications/bike.app")),
            InstallMode::OpenReleasePage,
            "APPIMAGE means nothing on macOS"
        );
    }

    #[test]
    fn the_installed_file_is_the_appimage_when_there_is_one() {
        let exe = Path::new("/tmp/.mount_bike/usr/bin/bike-osm-ride");
        let appimage = Path::new("/home/u/Apps/bike-osm-ride-0.2.0-linux-x86_64.AppImage");
        assert_eq!(
            install_target(Platform::Linux, exe, Some(appimage)),
            appimage,
            "inside the mount the exe is read-only, the AppImage is the real file"
        );
        assert_eq!(install_target(Platform::Linux, exe, None), exe);
        let win = Path::new("C:/apps/bike-osm-ride.exe");
        assert_eq!(install_target(Platform::Windows, win, None), win);
        assert_eq!(
            install_target(Platform::Windows, win, Some(appimage)),
            win,
            "APPIMAGE is ignored where it cannot exist"
        );
    }

    #[test]
    fn only_the_magic_of_the_platform_is_accepted() {
        fn padded(magic: &[u8]) -> Vec<u8> {
            let mut bytes = magic.to_vec();
            bytes.resize(MIN_PROGRAM_BYTES * 2, 0);
            bytes
        }
        let exe = padded(b"MZ");
        let elf = padded(b"\x7fELF");
        assert!(looks_like_program(Platform::Windows, &exe));
        assert!(!looks_like_program(Platform::Linux, &exe));
        assert!(looks_like_program(Platform::Linux, &elf));
        assert!(!looks_like_program(Platform::Windows, &elf));
        // an HTML error page is neither, and a tiny file is not a program
        assert!(!looks_like_program(
            Platform::Windows,
            &padded(b"<!doctype html>")
        ));
        assert!(!looks_like_program(Platform::Windows, b"MZ"));
        assert!(!looks_like_program(Platform::MacOs, &elf));
        assert!(!looks_like_program(Platform::Android, &exe));
    }

    #[test]
    fn the_old_file_sits_beside_the_executable() {
        let exe = Path::new("C:/apps/bike-osm-ride.exe");
        assert_eq!(old_path(exe), Path::new("C:/apps/bike-osm-ride.exe.old"));
        let appimage = Path::new("/home/u/bike-osm-ride-0.2.0-linux-x86_64.AppImage");
        assert_eq!(
            old_path(appimage),
            Path::new("/home/u/bike-osm-ride-0.2.0-linux-x86_64.AppImage.old")
        );
        assert_eq!(
            staged_path(appimage),
            Path::new("/home/u/bike-osm-ride-0.2.0-linux-x86_64.new")
        );
    }

    #[test]
    fn the_manual_hint_names_the_package_of_the_platform() {
        assert!(manual_install_hint(Platform::Linux).contains(".deb"));
        assert!(manual_install_hint(Platform::MacOs).contains(".dmg"));
        assert!(manual_install_hint(Platform::Android).contains("APK"));
        for platform in [
            Platform::Windows,
            Platform::Android,
            Platform::Linux,
            Platform::MacOs,
        ] {
            assert!(!manual_install_hint(platform).is_empty());
        }
    }
}
