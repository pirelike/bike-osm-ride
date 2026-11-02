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
    fn matches(self, asset_name: &str) -> bool {
        let name = asset_name.to_ascii_lowercase();
        match self {
            Platform::Windows => name.ends_with(".exe") && name.contains("windows"),
            Platform::Android => name.ends_with(".apk"),
            // The AppImage is the only Linux package the updater can install itself; the .deb
            // is offered on the release page instead.
            Platform::Linux => name.ends_with(".appimage") && name.contains("linux"),
            Platform::MacOs => name.ends_with(".dmg") && name.contains("macos"),
        }
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
    }
    // Inside an AppImage the running executable is in the read-only mount; the file the update
    // replaced is the `.AppImage` itself.
    if let Some(appimage) = std::env::var_os("APPIMAGE").map(PathBuf::from) {
        let _ = std::fs::remove_file(old_path(&appimage));
    }
}

/// Downloads the new program and swaps it in (Windows, or Linux inside an AppImage). On success
/// the caller starts the returned file and exits.
pub async fn install(update: &UpdateInfo, platform: Platform) -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let appimage = std::env::var_os("APPIMAGE").map(PathBuf::from);
    let target = install_target(platform, &exe, appimage.as_deref());

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
    // An exe starts with "MZ", an AppImage with "\x7fELF"; anything else (an HTML error page, a
    // link to a login wall) is not installed.
    if !looks_like_program(platform, &bytes) {
        return Err("a letöltött fájl nem futtatható program".into());
    }

    let staged = staged_path(&target);
    tokio::fs::write(&staged, &bytes)
        .await
        .map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // The download is written with the umask, and the AppImage must stay executable.
        tokio::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755))
            .await
            .map_err(|e| e.to_string())?;
    }
    let old = old_path(&target);
    let _ = std::fs::remove_file(&old);
    std::fs::rename(&target, &old).map_err(|e| e.to_string())?;
    if let Err(e) = std::fs::rename(&staged, &target) {
        // put the running version back so the app still starts next time
        let _ = std::fs::rename(&old, &target);
        return Err(e.to_string());
    }
    Ok(target)
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
        Platform::Linux => {
            "Nyisd meg a kiadás oldalát, töltsd le a .deb csomagot, és telepítsd \
             (sudo apt install ./bike-osm-ride-*.deb)."
        }
        Platform::MacOs => {
            "Nyisd meg a kiadás oldalát, töltsd le a .dmg-t, és húzd a bike-osm-ride.app-ot \
             az Alkalmazások mappába."
        }
        Platform::Android => "Töltsd le és telepítsd az új APK-t.",
        Platform::Windows => "Töltsd le az új exe-t a kiadás oldaláról.",
    }
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
             "browser_download_url": "https://github.com/owner/bike-osm-ride/releases/download/v0.2.0/bike-osm-ride-0.2.0-windows-x64.exe"},
            {"name": "bike-osm-ride-0.2.0-linux-x86_64.AppImage",
             "browser_download_url": "https://github.com/owner/bike-osm-ride/releases/download/v0.2.0/bike-osm-ride-0.2.0-linux-x86_64.AppImage"},
            {"name": "bike-osm-ride-0.2.0-linux-amd64.deb",
             "browser_download_url": "https://github.com/owner/bike-osm-ride/releases/download/v0.2.0/bike-osm-ride-0.2.0-linux-amd64.deb"},
            {"name": "bike-osm-ride-0.2.0-macos-universal.dmg",
             "browser_download_url": "https://github.com/owner/bike-osm-ride/releases/download/v0.2.0/bike-osm-ride-0.2.0-macos-universal.dmg"}
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
            "https://github.com/owner/bike-osm-ride/releases/download/v0.2.0/bike-osm-ride-0.2.0-windows-x64.exe",
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
