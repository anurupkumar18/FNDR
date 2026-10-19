//! What Files search reads, and what it never reads. Ported from DigUp's
//! `Rules`, whose defaults come from a real developer's Mac: Spotlight typed
//! 57k TypeScript files as movies, one repo held a 55k-clip dataset, and
//! screenshot tools moved screenshots into code projects.

use super::{FileKind, IndexOptions};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::Path;
use std::sync::LazyLock;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Classification {
    Index(FileKind),
    /// Left out for a reason worth reporting ("code or data file").
    Skip(&'static str),
    /// A type Files search does not handle at all.
    Ignore,
}

/// Folders never worth walking into. Hidden folders are skipped by the walk itself.
const SKIPPED_FOLDER_NAMES: &[&str] = &[
    "node_modules",
    "bower_components",
    "venv",
    "site-packages",
    "__pycache__",
    "DerivedData",
    "Pods",
    "Carthage",
];

/// Any of these in a folder makes it a code project, where only screenshots are read.
const PROJECT_MARKERS: &[&str] = &[
    ".git",
    "package.json",
    "Package.swift",
    "pyproject.toml",
    "setup.py",
    "requirements.txt",
    "Cargo.toml",
    "go.mod",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "Gemfile",
    "composer.json",
    "CMakeLists.txt",
];
const PROJECT_BUNDLE_EXTENSIONS: &[&str] = &["xcodeproj", "xcworkspace"];

/// Mac bundles: one thing to the Finder, skipped whole.
pub const PACKAGE_EXTENSIONS: &[&str] = &[
    "app",
    "framework",
    "bundle",
    "plugin",
    "appex",
    "xpc",
    "kext",
    "xcodeproj",
    "xcworkspace",
    "playground",
    "photoslibrary",
    "musiclibrary",
    "tvlibrary",
    "imovielibrary",
    "fcpbundle",
    "logicx",
    "band",
    "pkg",
    "dmg",
];

const DOC_EXTENSIONS: &[&str] = &[
    "txt", "text", "md", "markdown", "rtf", "docx", "doc", "odt", "html", "htm",
];

/// Source code and data files: off by default (an editor searches them better, and they flood an index).
const CODE_EXTENSIONS: &[&str] = &[
    "py", "ipynb", "c", "h", "cc", "cpp", "cxx", "hpp", "hh", "m", "mm", "swift", "tsx", "cts",
    "js", "jsx", "mjs", "cjs", "go", "rs", "java", "kt", "kts", "scala", "rb", "php", "cs", "fs",
    "sh", "bash", "zsh", "fish", "ps1", "lua", "pl", "r", "jl", "dart", "sql", "vue", "svelte",
    "css", "scss", "sass", "less", "json", "jsonl", "yaml", "yml", "toml", "xml", "plist",
    "gradle", "cmake", "mk", "lock", "log", "csv", "tsv", "ini", "cfg", "conf", "proto", "graphql",
    "tf", "nix", "zig", "hs", "ex", "exs", "erl", "clj", "elm", "ml", "vim", "el", "asm", "metal",
    "glsl", "hlsl", "wgsl", "cu", "pyi", "pyc",
];

/// Source files code search reads.
const SOURCE_EXTENSIONS: &[&str] = &[
    "py", "pyi", "pyx", "c", "h", "cc", "cpp", "cxx", "hpp", "hh", "hxx", "m", "mm", "swift", "ts",
    "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs", "go", "rs", "java", "kt", "kts", "scala",
    "groovy", "rb", "php", "cs", "fs", "sh", "bash", "zsh", "fish", "ps1", "lua", "pl", "pm", "r",
    "jl", "dart", "sql", "vue", "svelte", "astro", "html", "htm", "css", "scss", "sass", "less",
    "gradle", "cmake", "mk", "proto", "graphql", "gql", "tf", "hcl", "nix", "zig", "hs", "ex",
    "exs", "erl", "clj", "cljs", "elm", "ml", "mli", "vim", "el", "lisp", "asm", "s", "metal",
    "glsl", "hlsl", "wgsl", "cu", "cuh", "ino", "v", "sv", "tex",
];
/// What goes with code: a repo's docs and its settings.
const CODE_TEXT_EXTENSIONS: &[&str] = &[
    "md", "markdown", "rst", "adoc", "org", "txt", "yaml", "yml", "toml",
];
/// Code known by its name alone.
const CODE_FILE_NAMES: &[&str] = &[
    "Makefile",
    "makefile",
    "GNUmakefile",
    "Dockerfile",
    "Containerfile",
    "Justfile",
    "justfile",
    "Rakefile",
    "Gemfile",
    "Podfile",
    "Brewfile",
    "Procfile",
    "Vagrantfile",
    "Jenkinsfile",
];
/// Other people's code kept in a repo, and what builds and runs leave.
const CODE_SKIPPED_FOLDER_NAMES: &[&str] = &[
    "vendor",
    "Vendor",
    "vendored",
    "third_party",
    "third-party",
    "thirdparty",
    "3rdparty",
    "external",
    "extern",
    "deps",
    "build",
    "Build",
    "dist",
    "out",
    "target",
    "obj",
    "coverage",
    "htmlcov",
    "generated",
    "__generated__",
    "wandb",
    "mlruns",
    "lightning_logs",
];

const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "jpe", "jfif", "png", "gif", "bmp", "tif", "tiff", "webp", "heic", "heif",
    "avif", "psd", "tga", "pnm", "ppm", "pgm", "cr2", "nef", "arw", "dng", "raf", "orf", "rw2",
];
const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "m4a", "m4b", "aac", "wav", "wave", "flac", "ogg", "oga", "opus", "aiff", "aif", "wma",
    "amr", "caf", "mka",
];
const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "mov", "mkv", "webm", "avi", "wmv", "flv", "3gp", "3g2", "mpg", "mpeg", "m2v",
    "ogv", "mxf",
];

const SCREENSHOT_PREFIXES: &[&str] = &["Screenshot", "Screen Shot", "CleanShot", "SCR-"];
const SECRET_EXTENSIONS: &[&str] = &[
    "pem",
    "key",
    "p12",
    "pfx",
    "keystore",
    "jks",
    "ppk",
    "kdbx",
    "gpg",
    "asc",
    "crt",
    "cer",
    "der",
    "p8",
    "mobileprovision",
];
const SECRET_PREFIXES: &[&str] = &[
    "id_rsa",
    "id_dsa",
    "id_ecdsa",
    "id_ed25519",
    ".env",
    "secret",
    "credential",
    ".netrc",
    ".npmrc",
    ".pypirc",
    "htpasswd",
    ".htpasswd",
];
const CAMERA_PREFIXES: &[&str] = &[
    "img",
    "dsc",
    "pxl",
    "mvimg",
    "vid",
    "gopr",
    "dji",
    "photo",
    "whatsapp",
    "screenshot",
];

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_lowercase()
}

fn file_name(path: &Path) -> &str {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
}

/// What the main index reads, by extension (macOS's types come from the extension too for files that have one).
pub fn classify(path: &Path, options: &IndexOptions) -> Classification {
    let ext = extension(path);
    let ext = ext.as_str();
    if options
        .excluded_extensions
        .iter()
        .any(|excluded| excluded == ext)
    {
        return Classification::Skip("excluded type");
    }
    if is_secret(file_name(path)) {
        return Classification::Skip("key or password file");
    }
    // .ts and .mts are usually TypeScript. Only a real transport stream is video.
    if matches!(ext, "ts" | "mts" | "m2ts") {
        if is_mpeg_transport_stream(path) {
            return Classification::Index(FileKind::Video);
        }
        return code_or_skip(options);
    }
    if DOC_EXTENSIONS.contains(&ext) {
        return Classification::Index(FileKind::Doc);
    }
    if CODE_EXTENSIONS.contains(&ext) || SOURCE_EXTENSIONS.contains(&ext) {
        return code_or_skip(options);
    }
    if ext == "pdf" {
        return Classification::Index(FileKind::Pdf);
    }
    if IMAGE_EXTENSIONS.contains(&ext) {
        let kind = if is_screenshot(path) {
            FileKind::Screenshot
        } else {
            FileKind::Image
        };
        return Classification::Index(kind);
    }
    if VIDEO_EXTENSIONS.contains(&ext) {
        return Classification::Index(FileKind::Video);
    }
    if AUDIO_EXTENSIONS.contains(&ext) {
        return Classification::Index(FileKind::Audio);
    }
    Classification::Ignore
}

fn code_or_skip(options: &IndexOptions) -> Classification {
    if options.include_code {
        Classification::Index(FileKind::Doc)
    } else {
        Classification::Skip("code or data file")
    }
}

/// What code search reads in the code folders: source files, a repo's docs and
/// settings, and notebooks. Never data, minified bundles, licenses, or anything
/// that looks like a key or a password file.
pub fn classify_code(path: &Path, options: &IndexOptions) -> Classification {
    let name = file_name(path);
    let ext = extension(path);
    let ext = ext.as_str();
    if options
        .excluded_extensions
        .iter()
        .any(|excluded| excluded == ext)
    {
        return Classification::Skip("excluded type");
    }
    if is_secret(name) {
        return Classification::Skip("key or password file");
    }
    let lower = name.to_lowercase();
    if [".min.js", ".min.mjs", ".min.css"]
        .iter()
        .any(|suffix| lower.ends_with(suffix))
    {
        return Classification::Skip("minified");
    }
    if ["license", "licence", "copying", "notice"]
        .iter()
        .any(|prefix| lower.starts_with(prefix))
    {
        return Classification::Skip("license");
    }
    if matches!(ext, "ts" | "mts") && is_mpeg_transport_stream(path) {
        return Classification::Ignore;
    }
    if SOURCE_EXTENSIONS.contains(&ext)
        || CODE_TEXT_EXTENSIONS.contains(&ext)
        || ext == "ipynb"
        || CODE_FILE_NAMES.contains(&name)
        || name.starts_with("Dockerfile")
    {
        return Classification::Index(FileKind::Code);
    }
    if CODE_EXTENSIONS.contains(&ext) {
        return Classification::Skip("data file");
    }
    Classification::Ignore
}

/// Keys, certificates, password and credential files: never read, wherever they are.
pub fn is_secret(name: &str) -> bool {
    let lower = name.to_lowercase();
    let ext = Path::new(&lower)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("");
    SECRET_EXTENSIONS.contains(&ext)
        || SECRET_PREFIXES
            .iter()
            .any(|prefix| lower.starts_with(prefix))
}

/// MPEG-TS packets start with 0x47 every 188 bytes (or every 192 with a timecode, as in .m2ts).
pub fn is_mpeg_transport_stream(path: &Path) -> bool {
    let mut bytes = [0_u8; 600];
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut read = 0;
    while read < bytes.len() {
        match file.read(&mut bytes[read..]) {
            Ok(0) | Err(_) => break,
            Ok(count) => read += count,
        }
    }
    if read < 400 {
        return false;
    }
    (bytes[0] == 0x47 && bytes[188] == 0x47 && bytes[376] == 0x47)
        || (bytes[4] == 0x47 && bytes[196] == 0x47 && bytes[388] == 0x47)
}

/// macOS marks screenshots with an extended attribute that survives moves and copies; names are the fallback.
pub fn is_screenshot(path: &Path) -> bool {
    has_screenshot_marker(path)
        || SCREENSHOT_PREFIXES
            .iter()
            .any(|prefix| file_name(path).starts_with(prefix))
}

#[cfg(target_os = "macos")]
fn has_screenshot_marker(path: &Path) -> bool {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let Ok(path) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    let name = c"com.apple.metadata:kMDItemIsScreenCapture";
    // SAFETY: both pointers are valid NUL-terminated strings; a null buffer asks only for the size.
    unsafe { libc::getxattr(path.as_ptr(), name.as_ptr(), std::ptr::null_mut(), 0, 0, 0) > 0 }
}

#[cfg(not(target_os = "macos"))]
fn has_screenshot_marker(_path: &Path) -> bool {
    false
}

/// Why the walk does not go into a folder, or `None`.
pub fn skip_reason_for_folder(
    path: &Path,
    home: &Path,
    excluded: &[String],
) -> Option<&'static str> {
    let name = file_name(path);
    if SKIPPED_FOLDER_NAMES.contains(&name) {
        return Some("developer folder");
    }
    if name.ends_with(".noindex") {
        return Some(".noindex folder");
    }
    if path == home.join("Library") {
        return Some("~/Library");
    }
    let text = path.to_string_lossy();
    if excluded
        .iter()
        .any(|folder| text == folder.as_str() || text.starts_with(&format!("{folder}/")))
    {
        return Some("excluded folder");
    }
    None
}

/// Why code search does not go into a folder, or `None`.
pub fn code_skip_reason_for_folder(path: &Path) -> Option<&'static str> {
    if CODE_SKIPPED_FOLDER_NAMES.contains(&file_name(path)) {
        return Some("vendored or built code");
    }
    // A submodule or a worktree (".git" is a file there): someone else's code, or a copy of a repo's own.
    if path.join(".git").is_file() {
        return Some("submodule or worktree");
    }
    None
}

pub fn is_package(path: &Path) -> bool {
    PACKAGE_EXTENSIONS.contains(&extension(path).as_str())
}

pub fn is_project_root(folder: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        PROJECT_MARKERS.contains(&name.as_ref())
            || PROJECT_BUNDLE_EXTENSIONS.contains(&extension(Path::new(name.as_ref())).as_str())
    })
}

/// Whether a change the folder watcher reports at `path` is where the walk of
/// `root` goes: not inside a hidden or skipped folder. Anything else would ask
/// for a sync that finds nothing new, FNDR's own writes under `~/Library` included.
pub fn is_walked(path: &Path, root: &Path, home: &Path, excluded: &[String]) -> bool {
    let Ok(rest) = path.strip_prefix(root) else {
        return false;
    };
    let mut folder = root.to_path_buf();
    for part in rest.components() {
        folder.push(part);
        let hidden = part.as_os_str().to_string_lossy().starts_with('.');
        if hidden || skip_reason_for_folder(&folder, home, excluded).is_some() {
            return false;
        }
    }
    true
}

static HEX_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[0-9a-f]{8,}([-_][0-9a-f]{4,})*$").unwrap());
static NUMBERED_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z]{0,16}[-_. ]?\d{3,}([-_.]\d+)*$").unwrap());

/// Machine-made names like `clip_000123`, `frame-0042` or `3f9a1c0e-...`, but not camera names like `IMG_1234`.
pub fn looks_machine_named(stem: &str) -> bool {
    let name = stem.to_lowercase();
    if CAMERA_PREFIXES
        .iter()
        .any(|prefix| name.starts_with(prefix))
    {
        return false;
    }
    HEX_NAME.is_match(&name) || NUMBERED_NAME.is_match(&name)
}

/// A file the dataset check looks at: its folder, its name without extension, its kind.
pub struct DatasetEntry<'a> {
    pub folder: &'a str,
    pub stem: &'a str,
    pub kind: FileKind,
}

/// Folders with hundreds of same-kind, machine-named media files: training data,
/// not the person's things. Skipped and reported. A chosen root is never a dataset.
pub fn dataset_folders(
    files: &[DatasetEntry<'_>],
    roots: &HashSet<String>,
    minimum: usize,
) -> HashMap<String, usize> {
    let mut groups: HashMap<(&str, FileKind), Vec<&str>> = HashMap::new();
    for file in files
        .iter()
        .filter(|file| file.kind.is_media() && !roots.contains(file.folder))
    {
        groups
            .entry((file.folder, file.kind))
            .or_default()
            .push(file.stem);
    }
    let mut datasets = HashMap::new();
    for ((folder, _), stems) in groups {
        if stems.len() < minimum {
            continue;
        }
        let machine = stems
            .iter()
            .filter(|stem| looks_machine_named(stem))
            .count();
        if machine as f64 / stems.len() as f64 >= 0.7 {
            *datasets.entry(folder.to_string()).or_insert(0) += stems.len();
        }
    }
    datasets
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn classify_name(name: &str) -> Classification {
        classify(Path::new(name), &IndexOptions::default())
    }

    #[test]
    fn secrets_are_never_read() {
        for name in [
            "id_rsa",
            "id_ed25519.pub",
            ".env.local",
            "server.pem",
            "cert.p12",
            "credentials.json",
            "secrets.txt",
            "vault.kdbx",
            ".netrc",
        ] {
            assert!(is_secret(name), "{name}");
            assert_eq!(
                classify_name(name),
                Classification::Skip("key or password file"),
                "{name}"
            );
            assert_eq!(
                classify_code(Path::new(name), &IndexOptions::default()),
                Classification::Skip("key or password file")
            );
        }
        assert!(!is_secret("lease.pdf"));
    }

    #[test]
    fn files_are_sorted_by_kind() {
        assert_eq!(
            classify_name("lease.pdf"),
            Classification::Index(FileKind::Pdf)
        );
        assert_eq!(
            classify_name("notes.md"),
            Classification::Index(FileKind::Doc)
        );
        assert_eq!(
            classify_name("beach.HEIC"),
            Classification::Index(FileKind::Image)
        );
        assert_eq!(
            classify_name("Screenshot 2026-10-01 at 9.41.png"),
            Classification::Index(FileKind::Screenshot)
        );
        assert_eq!(
            classify_name("CleanShot 1.png"),
            Classification::Index(FileKind::Screenshot)
        );
        assert_eq!(
            classify_name("talk.m4a"),
            Classification::Index(FileKind::Audio)
        );
        assert_eq!(
            classify_name("zebra.mov"),
            Classification::Index(FileKind::Video)
        );
        assert_eq!(
            classify_name("main.rs"),
            Classification::Skip("code or data file")
        );
        assert_eq!(
            classify_name("export.csv"),
            Classification::Skip("code or data file")
        );
        assert_eq!(classify_name("thing.xyz"), Classification::Ignore);
        let with_code = IndexOptions {
            include_code: true,
            ..IndexOptions::default()
        };
        assert_eq!(
            classify(Path::new("main.rs"), &with_code),
            Classification::Index(FileKind::Doc)
        );
        let excluded = IndexOptions {
            excluded_extensions: vec!["pdf".into()],
            ..IndexOptions::default()
        };
        assert_eq!(
            classify(Path::new("a.pdf"), &excluded),
            Classification::Skip("excluded type")
        );
    }

    #[test]
    fn typescript_is_not_video_but_a_transport_stream_is() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("app.ts");
        std::fs::write(&script, "export const a = 1;\n".repeat(40)).unwrap();
        assert_eq!(
            classify(&script, &IndexOptions::default()),
            Classification::Skip("code or data file")
        );
        let mut packets = vec![0_u8; 188 * 4];
        for packet in 0..4 {
            packets[packet * 188] = 0x47;
        }
        let stream = dir.path().join("clip.ts");
        std::fs::write(&stream, packets).unwrap();
        assert_eq!(
            classify(&stream, &IndexOptions::default()),
            Classification::Index(FileKind::Video)
        );
        assert_eq!(
            classify_code(&stream, &IndexOptions::default()),
            Classification::Ignore
        );
    }

    #[test]
    fn code_search_reads_source_and_docs_but_not_data() {
        let options = IndexOptions::default();
        let code = |name: &str| classify_code(Path::new(name), &options);
        assert_eq!(code("retry.py"), Classification::Index(FileKind::Code));
        assert_eq!(code("README.md"), Classification::Index(FileKind::Code));
        assert_eq!(
            code("Dockerfile.dev"),
            Classification::Index(FileKind::Code)
        );
        assert_eq!(
            code("analysis.ipynb"),
            Classification::Index(FileKind::Code)
        );
        assert_eq!(code("bundle.min.js"), Classification::Skip("minified"));
        assert_eq!(code("LICENSE"), Classification::Skip("license"));
        assert_eq!(code("package-lock.json"), Classification::Skip("data file"));
        assert_eq!(code("photo.png"), Classification::Ignore);
    }

    #[test]
    fn skipped_folders() {
        let home = PathBuf::from("/Users/a");
        let reason = |path: &str| {
            skip_reason_for_folder(Path::new(path), &home, &["/Users/a/Private".to_string()])
        };
        assert_eq!(reason("/Users/a/x/node_modules"), Some("developer folder"));
        assert_eq!(reason("/Users/a/Photos.noindex"), Some(".noindex folder"));
        assert_eq!(reason("/Users/a/Library"), Some("~/Library"));
        assert_eq!(reason("/Users/a/Private/taxes"), Some("excluded folder"));
        assert_eq!(reason("/Users/a/PrivateNotes"), None);
        assert_eq!(reason("/Users/a/Documents"), None);
        assert_eq!(
            code_skip_reason_for_folder(Path::new("/r/third_party")),
            Some("vendored or built code")
        );
        assert!(is_package(Path::new("/Applications/FNDR.app")));
    }

    #[test]
    fn watcher_changes_count_only_where_the_walk_goes() {
        let home = PathBuf::from("/Users/a");
        let root = PathBuf::from("/Users/a");
        assert!(is_walked(
            Path::new("/Users/a/Documents/x.pdf"),
            &root,
            &home,
            &[]
        ));
        assert!(!is_walked(
            Path::new("/Users/a/Library/Application Support/FNDR/files.sqlite"),
            &root,
            &home,
            &[]
        ));
        assert!(!is_walked(
            Path::new("/Users/a/.ssh/id_rsa"),
            &root,
            &home,
            &[]
        ));
        assert!(!is_walked(
            Path::new("/Users/a/p/node_modules/a.js"),
            &root,
            &home,
            &[]
        ));
        assert!(!is_walked(Path::new("/elsewhere/a.pdf"), &root, &home, &[]));
    }

    #[test]
    fn project_roots_are_found_by_their_markers() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_project_root(dir.path()));
        std::fs::write(dir.path().join("Cargo.toml"), "").unwrap();
        assert!(is_project_root(dir.path()));
    }

    #[test]
    fn machine_names_but_not_camera_names() {
        for name in [
            "clip_000123",
            "frame-0042",
            "3f9a1c0e-1b2c",
            "000451",
            "sample.0001",
        ] {
            assert!(looks_machine_named(name), "{name}");
        }
        for name in [
            "IMG_1234",
            "DSC01234",
            "PXL_20260101_123",
            "beach",
            "lease final",
        ] {
            assert!(!looks_machine_named(name), "{name}");
        }
    }

    #[test]
    fn datasets_need_many_machine_named_media_files_outside_a_root() {
        let stems: Vec<String> = (0..300).map(|n| format!("clip_{n:06}")).collect();
        let entries: Vec<DatasetEntry<'_>> = stems
            .iter()
            .map(|stem| DatasetEntry {
                folder: "/d/train",
                stem,
                kind: FileKind::Image,
            })
            .collect();
        let found = dataset_folders(&entries, &HashSet::new(), 300);
        assert_eq!(found.get("/d/train"), Some(&300));
        let roots: HashSet<String> = ["/d/train".to_string()].into();
        assert!(dataset_folders(&entries, &roots, 300).is_empty());
        assert!(dataset_folders(&entries[..299], &HashSet::new(), 300).is_empty());
    }
}
