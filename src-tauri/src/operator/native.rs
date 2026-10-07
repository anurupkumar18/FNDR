//! FNDR-native steps: open an app or a link with NSWorkspace, wait until an
//! app is in front, and read what is in front and what is playing.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::plan::Observation;

/// Folders an app name is looked up in, in order.
fn app_roots() -> Vec<PathBuf> {
    let mut roots = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
        PathBuf::from("/System/Applications/Utilities"),
        PathBuf::from("/Applications/Utilities"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join("Applications"));
    }
    roots
}

fn bundle_stem(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    name.strip_suffix(".app").map(str::to_lowercase)
}

/// The `.app` a spoken name means: an exact name first ("Spotify"), then one
/// that starts with it ("Chrome" never matches; "Google" finds "Google Chrome"),
/// then one that contains it as a word ("Chrome" finds "Google Chrome").
pub fn resolve_app(name: &str, roots: &[PathBuf]) -> Option<PathBuf> {
    let wanted = name.trim().trim_end_matches(".app").to_lowercase();
    if wanted.is_empty() {
        return None;
    }
    let bundles: Vec<PathBuf> = roots
        .iter()
        .filter_map(|root| std::fs::read_dir(root).ok())
        .flat_map(|entries| entries.filter_map(Result::ok).map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "app"))
        .collect();
    let stem = |path: &&PathBuf| bundle_stem(path).unwrap_or_default();
    bundles
        .iter()
        .find(|path| stem(path) == wanted)
        .or_else(|| {
            bundles
                .iter()
                .find(|path| stem(path).starts_with(&format!("{wanted} ")))
        })
        .or_else(|| {
            bundles
                .iter()
                .find(|path| stem(path).split_whitespace().any(|word| word == wanted))
        })
        .cloned()
}

#[cfg(target_os = "macos")]
fn workspace_open(url: &objc2_foundation::NSURL) -> bool {
    unsafe { objc2_app_kit::NSWorkspace::sharedWorkspace().openURL(url) }
}

/// Launches or focuses an app. Returns the bundle path that was opened.
pub fn open_app(name: &str) -> Result<PathBuf, String> {
    let path = resolve_app(name, &app_roots())
        .ok_or_else(|| format!("No app called {name} is installed."))?;
    #[cfg(target_os = "macos")]
    {
        use objc2_foundation::{NSString, NSURL};
        let url = unsafe { NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy())) };
        if !workspace_open(&url) {
            return Err(format!("macOS would not open {name}."));
        }
    }
    Ok(path)
}

/// Opens an http(s) link in the default browser.
pub fn open_url(url: &str) -> Result<(), String> {
    let lower = url.trim().to_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return Err("Only web links can be opened.".to_string());
    }
    #[cfg(target_os = "macos")]
    {
        use objc2_foundation::{NSString, NSURL};
        let parsed = unsafe { NSURL::URLWithString(&NSString::from_str(url.trim())) }
            .ok_or_else(|| "That link is not valid.".to_string())?;
        if !workspace_open(&parsed) {
            return Err("macOS would not open that link.".to_string());
        }
    }
    Ok(())
}

/// What is in front right now, plus playback for a media app.
pub async fn observe(media_app: Option<&str>) -> Observation {
    let (media_playing, media_track) = match media_app {
        Some(app) => media_state(app).await,
        None => (None, None),
    };
    let mut seen = frontmost();
    seen.media_playing = media_playing;
    seen.media_track = media_track;
    seen
}

/// `(pid, name, bundle id)` from `lsappinfo info -only name -only bundleid -only pid`.
pub(crate) fn parse_lsappinfo(out: &str) -> Option<(i32, String, String)> {
    let quoted = |text: &str| -> Option<String> {
        let start = text.find('"')? + 1;
        let end = text[start..].find('"')? + start;
        Some(text[start..end].to_string())
    };
    let name = out
        .lines()
        .next()
        .filter(|line| line.starts_with('"'))
        .and_then(quoted)?;
    let bundle = out
        .lines()
        .find_map(|line| line.trim().strip_prefix("bundleID=").and_then(quoted))
        .unwrap_or_default();
    let pid = out
        .split("pid = ")
        .nth(1)?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    Some((pid, name, bundle))
}

/// Which app LaunchServices has in front. Current even in a process whose main
/// run loop is not spinning, where `NSWorkspace.frontmostApplication` goes stale.
fn lsappinfo_front() -> Option<(i32, String, String)> {
    let run = |args: &[&str]| -> Option<String> {
        let output = std::process::Command::new("/usr/bin/lsappinfo")
            .args(args)
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
    };
    let asn = run(&["front"])?;
    parse_lsappinfo(&run(&[
        "info", "-only", "name", "-only", "bundleid", "-only", "pid", &asn,
    ])?)
}

/// The app in front and its window, through Accessibility when allowed (always
/// current), else through NSWorkspace.
fn frontmost() -> Observation {
    let front = lsappinfo_front();
    #[cfg(target_os = "macos")]
    if let Some((pid, window)) = crate::accessibility::ax_frontmost() {
        let agrees = front
            .as_ref()
            .is_none_or(|(front_pid, _, _)| *front_pid == pid);
        let app = unsafe {
            objc2_app_kit::NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
        };
        if let (true, Some(app)) = (agrees, app) {
            let (name, bundle) = unsafe {
                (
                    app.localizedName()
                        .map(|s| s.to_string())
                        .unwrap_or_default(),
                    app.bundleIdentifier()
                        .map(|s| s.to_string())
                        .unwrap_or_default(),
                )
            };
            let browser_url = window
                .document_url
                .filter(|url| url.starts_with("http://") || url.starts_with("https://"));
            return Observation {
                frontmost_app: name,
                frontmost_bundle: bundle,
                window_title: window.title.unwrap_or_default(),
                browser_url,
                ..Default::default()
            };
        }
    }
    if let Some((_, name, bundle)) = front {
        // The app is known; its window could not be read in this process.
        return Observation {
            frontmost_app: name,
            frontmost_bundle: bundle,
            ..Default::default()
        };
    }
    let front = crate::capture::macos::get_frontmost_app_info_fresh();
    Observation {
        frontmost_app: front.app_name,
        frontmost_bundle: front.bundle_id.unwrap_or_default(),
        window_title: front.window_title,
        browser_url: front.browser_url,
        ..Default::default()
    }
}

/// Polls until `app` is the frontmost app, or the timeout passes.
pub async fn wait_until_frontmost(app: &str, timeout: Duration) -> Observation {
    let wanted = app.trim().to_lowercase();
    let started = Instant::now();
    loop {
        let seen = observe(None).await;
        let name = seen.frontmost_app.to_lowercase();
        let in_front = name == wanted
            || name.contains(&wanted)
            || (!name.is_empty() && wanted.contains(&name))
            || seen.frontmost_bundle.to_lowercase() == wanted;
        if in_front || started.elapsed() >= timeout {
            return seen;
        }
        tokio::time::sleep(Duration::from_millis(120)).await;
    }
}

/// Longest FNDR waits on an Apple Event. An unanswered Automation prompt
/// would otherwise hold a step for the system's two-minute event timeout.
const APPLE_EVENT_TIMEOUT: Duration = Duration::from_secs(4);

async fn osascript(script: &str) -> Result<std::process::Output, String> {
    let child = tokio::process::Command::new("/usr/bin/osascript")
        .args(["-e", script])
        .kill_on_drop(true)
        .output();
    tokio::time::timeout(APPLE_EVENT_TIMEOUT, child)
        .await
        .map_err(|_| "The app did not answer in time.".to_string())?
        .map_err(|e| e.to_string())
}

/// Scriptable players FNDR can ask "are you playing, and what".
fn media_script_target(app: &str) -> Option<&'static str> {
    match app.trim().to_lowercase().as_str() {
        "spotify" | "com.spotify.client" => Some("Spotify"),
        "music" | "apple music" | "com.apple.music" => Some("Music"),
        _ => None,
    }
}

/// `(playing, track)` from the player's AppleScript dictionary. The first
/// call asks the person for Automation permission; a refusal reads as unknown.
pub async fn media_state(app: &str) -> (Option<bool>, Option<String>) {
    let Some(target) = media_script_target(app) else {
        return (None, None);
    };
    let script = format!(
        "if application \"{target}\" is running then tell application \"{target}\" to return (player state as text) & linefeed & (name of current track)"
    );
    match osascript(&script).await {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout);
            let mut lines = text.lines();
            let state = lines.next().unwrap_or_default().trim().to_lowercase();
            let track = lines
                .next()
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_string);
            if state.is_empty() {
                (Some(false), None)
            } else {
                (Some(state == "playing"), track)
            }
        }
        Ok(output) => {
            tracing::warn!(
                stderr = %String::from_utf8_lossy(&output.stderr).trim(),
                "operator:media_state_unreadable"
            );
            (None, None)
        }
        Err(error) => {
            tracing::warn!(%error, "operator:media_state_failed");
            (None, None)
        }
    }
}

/// Whether FNDR may send Apple Events to `app` (Automation), checked by asking
/// for something harmless. `None` when the app is not running.
pub async fn automation_allowed(app: &str) -> Option<bool> {
    let script = format!(
        "if application \"{app}\" is running then tell application \"{app}\" to return name"
    );
    let output = osascript(&script).await.ok()?;
    if output.status.success() {
        let running = !String::from_utf8_lossy(&output.stdout).trim().is_empty();
        running.then_some(true)
    } else {
        Some(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_apps(names: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for name in names {
            std::fs::create_dir_all(dir.path().join(format!("{name}.app"))).unwrap();
        }
        dir
    }

    #[test]
    fn resolves_spoken_app_names_to_bundles() {
        let dir = fake_apps(&["Spotify", "Google Chrome", "Notes", "Spotify Helper"]);
        let roots = vec![dir.path().to_path_buf()];
        let stem = |name: &str| resolve_app(name, &roots).and_then(|p| bundle_stem(&p));
        assert_eq!(stem("spotify").as_deref(), Some("spotify"));
        assert_eq!(stem("Chrome").as_deref(), Some("google chrome"));
        assert_eq!(stem("Google").as_deref(), Some("google chrome"));
        assert_eq!(stem("Notes.app").as_deref(), Some("notes"));
        assert_eq!(stem("Photoshop"), None);
        assert_eq!(stem(""), None);
    }

    #[test]
    fn only_known_players_are_scripted() {
        assert_eq!(media_script_target("Spotify"), Some("Spotify"));
        assert_eq!(media_script_target("com.apple.music"), Some("Music"));
        assert_eq!(media_script_target("Notes"), None);
    }

    #[test]
    fn reads_the_front_app_from_lsappinfo() {
        let out = "\"Dia\" ASN:0x0-0x3d63d6: (in front) \n    bundleID=\"company.thebrowser.dia\"\n    bundle path=[ NULL ] \n    pid = 45176 !cgsConnection type=[ NULL ]\n";
        assert_eq!(
            parse_lsappinfo(out),
            Some((
                45176,
                "Dia".to_string(),
                "company.thebrowser.dia".to_string()
            ))
        );
        assert_eq!(parse_lsappinfo("[ NULL ]  [ NULL ]\n    pid = 1\n"), None);
    }

    #[tokio::test]
    async fn unscriptable_apps_read_as_unknown_without_asking() {
        assert_eq!(media_state("Notes").await, (None, None));
    }

    #[test]
    fn non_web_links_are_refused_before_macos_sees_them() {
        assert!(open_url("file:///etc/hosts").is_err());
        assert!(open_url("javascript:alert(1)").is_err());
    }
}
