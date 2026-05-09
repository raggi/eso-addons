use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const STEAM_APP_ID: &str = "306130";
const ESO_DIR: &str = "Elder Scrolls Online";
const LIVE_DIR: &str = "live";
const ADDONS_DIR: &str = "AddOns";
const WINE_DOCS: &str = "drive_c/users/steamuser/My Documents";
const WINE_DOCS_USER: &str = "drive_c/users";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddonPathCandidate {
    pub path: PathBuf,
    pub source: AddonPathSource,
    pub status: AddonPathStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddonPathSource {
    NativeDocuments,
    OneDriveDocuments,
    SteamProton,
    SteamProtonFlatpak,
    SteamLibrary,
    Wineprefix,
    LutrisDefault,
    CrossoverBottle,
}

impl AddonPathSource {
    pub fn label(&self) -> &'static str {
        match self {
            Self::NativeDocuments => "Documents",
            Self::OneDriveDocuments => "OneDrive Documents",
            Self::SteamProton => "Steam (Proton)",
            Self::SteamProtonFlatpak => "Steam (Flatpak)",
            Self::SteamLibrary => "Steam Library",
            Self::Wineprefix => "Wine prefix ($WINEPREFIX)",
            Self::LutrisDefault => "Lutris",
            Self::CrossoverBottle => "CrossOver bottle",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AddonPathStatus {
    PrefixExists,
    GameSavedataExists,
    LiveExists,
    AddOnsExists,
}

impl AddonPathStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::AddOnsExists => "AddOns folder ready",
            Self::LiveExists => "live folder present, AddOns not yet created",
            Self::GameSavedataExists => "Game savedata present",
            Self::PrefixExists => "Wine prefix only",
        }
    }
}

struct Probe {
    home: PathBuf,
    documents: Option<PathBuf>,
    onedrive: Option<PathBuf>,
    wineprefix: Option<PathBuf>,
    user: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Platform {
    Linux,
    Macos,
    Windows,
}

const CURRENT_PLATFORM: Platform = if cfg!(target_os = "windows") {
    Platform::Windows
} else if cfg!(target_os = "macos") {
    Platform::Macos
} else {
    Platform::Linux
};

pub fn detect_candidates() -> Vec<AddonPathCandidate> {
    let probe = Probe {
        home: dirs::home_dir().unwrap_or_default(),
        documents: dirs::document_dir(),
        onedrive: env::var_os("OneDrive").map(PathBuf::from),
        wineprefix: env::var_os("WINEPREFIX").map(PathBuf::from),
        user: env::var("USER").or_else(|_| env::var("USERNAME")).ok(),
    };
    let mut out = collect(&probe, CURRENT_PLATFORM);
    rank_and_dedupe(&mut out);
    out
}

pub fn best_default() -> PathBuf {
    detect_candidates()
        .into_iter()
        .next()
        .map(|c| c.path)
        .unwrap_or_else(platform_default_path)
}

pub fn platform_default_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_default();
    if cfg!(target_os = "windows") {
        dirs::document_dir()
            .unwrap_or_else(|| home.join("Documents"))
            .join(ESO_DIR)
            .join(LIVE_DIR)
            .join(ADDONS_DIR)
    } else if cfg!(target_os = "macos") {
        dirs::document_dir()
            .unwrap_or_else(|| home.join("Documents"))
            .join(ESO_DIR)
            .join(LIVE_DIR)
            .join(ADDONS_DIR)
    } else {
        steam_proton_addons(&home.join(".local/share/Steam"))
    }
}

fn collect(probe: &Probe, platform: Platform) -> Vec<AddonPathCandidate> {
    let mut out = Vec::new();

    match platform {
        Platform::Windows => {
            if let Some(docs) = &probe.documents {
                push_candidate(
                    &mut out,
                    eso_under_documents(docs),
                    AddonPathSource::NativeDocuments,
                );
            }
            if let Some(od) = &probe.onedrive {
                push_candidate(
                    &mut out,
                    eso_under_documents(&od.join("Documents")),
                    AddonPathSource::OneDriveDocuments,
                );
                push_candidate(
                    &mut out,
                    eso_under_documents(od),
                    AddonPathSource::OneDriveDocuments,
                );
            }
        }
        Platform::Macos => {
            if let Some(docs) = &probe.documents {
                push_candidate(
                    &mut out,
                    eso_under_documents(docs),
                    AddonPathSource::NativeDocuments,
                );
            }
            for bottle in crossover_bottles(&probe.home) {
                for user_dir in wine_user_dirs(&bottle, probe.user.as_deref()) {
                    push_candidate(
                        &mut out,
                        eso_under_documents(&user_dir),
                        AddonPathSource::CrossoverBottle,
                    );
                }
            }
        }
        Platform::Linux => {
            for (root, install_source) in linux_steam_roots(&probe.home) {
                for (idx, lib) in steam_libraries(&root).into_iter().enumerate() {
                    let source = if idx == 0 {
                        install_source
                    } else {
                        AddonPathSource::SteamLibrary
                    };
                    push_candidate(&mut out, steam_proton_addons(&lib), source);
                }
            }
            if let Some(prefix) = &probe.wineprefix {
                for user_dir in wine_user_dirs(prefix, probe.user.as_deref()) {
                    push_candidate(
                        &mut out,
                        eso_under_documents(&user_dir),
                        AddonPathSource::Wineprefix,
                    );
                }
            }
            let lutris = probe.home.join("Games/elder-scrolls-online");
            if lutris.exists() {
                for user_dir in wine_user_dirs(&lutris, probe.user.as_deref()) {
                    push_candidate(
                        &mut out,
                        eso_under_documents(&user_dir),
                        AddonPathSource::LutrisDefault,
                    );
                }
            }
        }
    }

    out
}

fn linux_steam_roots(home: &Path) -> Vec<(PathBuf, AddonPathSource)> {
    let mut roots = Vec::new();
    let native = home.join(".local/share/Steam");
    if native.join("steamapps").is_dir() {
        roots.push((native, AddonPathSource::SteamProton));
    } else {
        let alt = home.join(".steam/steam");
        if alt.join("steamapps").is_dir() {
            roots.push((alt, AddonPathSource::SteamProton));
        }
    }
    let flatpak = home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam");
    if flatpak.join("steamapps").is_dir() {
        roots.push((flatpak, AddonPathSource::SteamProtonFlatpak));
    }
    roots
}

fn steam_libraries(steam_root: &Path) -> Vec<PathBuf> {
    let mut libs = vec![steam_root.to_path_buf()];
    let vdf = steam_root.join("steamapps/libraryfolders.vdf");
    if let Ok(contents) = fs::read_to_string(&vdf) {
        for path in parse_library_paths(&contents) {
            if path != steam_root {
                libs.push(path);
            }
        }
    }
    libs
}

fn parse_library_paths(vdf: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut rest = vdf;
    while let Some(idx) = rest.find("\"path\"") {
        rest = &rest[idx + "\"path\"".len()..];
        let Some(open) = rest.find('"') else { break };
        let after_open = &rest[open + 1..];
        let Some(close) = after_open.find('"') else { break };
        let raw = &after_open[..close];
        let unescaped = raw.replace("\\\\", "\\");
        out.push(PathBuf::from(unescaped));
        rest = &after_open[close + 1..];
    }
    out
}

fn steam_proton_addons(library: &Path) -> PathBuf {
    library
        .join("steamapps/compatdata")
        .join(STEAM_APP_ID)
        .join("pfx")
        .join(WINE_DOCS)
        .join(ESO_DIR)
        .join(LIVE_DIR)
        .join(ADDONS_DIR)
}

fn wine_user_dirs(prefix: &Path, user: Option<&str>) -> Vec<PathBuf> {
    let users = prefix.join(WINE_DOCS_USER);
    let mut out = Vec::new();
    let mut push = |name: &str| {
        let dir = users.join(name).join("My Documents");
        if !out.contains(&dir) {
            out.push(dir);
        }
    };
    push("steamuser");
    if let Some(u) = user {
        push(u);
    }
    push("crossover");
    push("user");
    out
}

fn crossover_bottles(home: &Path) -> Vec<PathBuf> {
    let bottles_root = home.join("Library/Application Support/CrossOver/Bottles");
    let Ok(entries) = fs::read_dir(&bottles_root) else {
        return Vec::new();
    };
    entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect()
}

fn eso_under_documents(documents: &Path) -> PathBuf {
    documents.join(ESO_DIR).join(LIVE_DIR).join(ADDONS_DIR)
}

fn classify(path: &Path) -> Option<AddonPathStatus> {
    if path.is_dir() {
        return Some(AddonPathStatus::AddOnsExists);
    }
    let live = path.parent()?;
    if live.is_dir() {
        return Some(AddonPathStatus::LiveExists);
    }
    let eso = live.parent()?;
    if eso.is_dir() {
        return Some(AddonPathStatus::GameSavedataExists);
    }
    let docs = eso.parent()?;
    if docs.is_dir() {
        return Some(AddonPathStatus::PrefixExists);
    }
    None
}

fn push_candidate(out: &mut Vec<AddonPathCandidate>, path: PathBuf, source: AddonPathSource) {
    let Some(status) = classify(&path) else {
        return;
    };
    out.push(AddonPathCandidate {
        path,
        source,
        status,
    });
}

fn rank_and_dedupe(candidates: &mut Vec<AddonPathCandidate>) {
    candidates.sort_by(|a, b| {
        b.status
            .cmp(&a.status)
            .then_with(|| source_priority(a.source).cmp(&source_priority(b.source)))
    });
    let mut seen: Vec<PathBuf> = Vec::new();
    candidates.retain(|c| {
        if seen.iter().any(|p| p == &c.path) {
            false
        } else {
            seen.push(c.path.clone());
            true
        }
    });
}

fn source_priority(s: AddonPathSource) -> u8 {
    match s {
        AddonPathSource::NativeDocuments => 0,
        AddonPathSource::OneDriveDocuments => 1,
        AddonPathSource::SteamProton => 2,
        AddonPathSource::SteamProtonFlatpak => 3,
        AddonPathSource::SteamLibrary => 4,
        AddonPathSource::CrossoverBottle => 5,
        AddonPathSource::Wineprefix => 6,
        AddonPathSource::LutrisDefault => 7,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn touch_dir(p: &Path) {
        fs::create_dir_all(p).unwrap();
    }

    fn write_file(p: &Path, contents: &str) {
        touch_dir(p.parent().unwrap());
        fs::write(p, contents).unwrap();
    }

    #[test]
    fn parse_libraryfolders_extracts_paths() {
        let vdf = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"/home/me/.local/share/Steam"
		"label"		""
	}
	"1"
	{
		"path"		"/mnt/games/SteamLibrary"
	}
}
"#;
        let paths = parse_library_paths(vdf);
        assert_eq!(
            paths,
            vec![
                PathBuf::from("/home/me/.local/share/Steam"),
                PathBuf::from("/mnt/games/SteamLibrary"),
            ]
        );
    }

    #[test]
    fn parse_libraryfolders_unescapes_backslashes() {
        let vdf = r#""path"		"C:\\Program Files\\Steam""#;
        let paths = parse_library_paths(vdf);
        assert_eq!(paths, vec![PathBuf::from(r"C:\Program Files\Steam")]);
    }

    #[test]
    fn classify_walks_up_existing_parents() {
        let tmp = TempDir::new().unwrap();
        let docs = tmp.path().join("My Documents");
        let eso = docs.join(ESO_DIR);
        let live = eso.join(LIVE_DIR);
        let addons = live.join(ADDONS_DIR);

        touch_dir(tmp.path());
        assert_eq!(classify(&addons), None);

        touch_dir(&docs);
        assert_eq!(classify(&addons), Some(AddonPathStatus::PrefixExists));

        touch_dir(&eso);
        assert_eq!(classify(&addons), Some(AddonPathStatus::GameSavedataExists));

        touch_dir(&live);
        assert_eq!(classify(&addons), Some(AddonPathStatus::LiveExists));

        touch_dir(&addons);
        assert_eq!(classify(&addons), Some(AddonPathStatus::AddOnsExists));
    }

    #[test]
    fn collect_finds_steam_proton_addons() {
        let tmp = TempDir::new().unwrap();
        let home = tmp.path();
        let lib = home.join(".local/share/Steam");
        let addons = steam_proton_addons(&lib);
        touch_dir(&addons);

        let probe = Probe {
            home: home.into(),
            documents: None,
            onedrive: None,
            wineprefix: None,
            user: None,
        };
        let mut got = collect(&probe, Platform::Linux);
        rank_and_dedupe(&mut got);
        assert!(got.iter().any(|c| c.path == addons
            && c.source == AddonPathSource::SteamProton
            && c.status == AddonPathStatus::AddOnsExists));
    }

    #[test]
    fn collect_picks_up_alt_steam_library() {
        let tmp = TempDir::new().unwrap();
        let home = tmp.path();
        let primary = home.join(".local/share/Steam");
        touch_dir(&primary.join("steamapps"));
        let alt = tmp.path().join("alt-library");
        let alt_addons = steam_proton_addons(&alt);
        touch_dir(&alt_addons);

        let vdf = format!(
            "\"libraryfolders\"\n{{\n\t\"0\" {{ \"path\" \"{}\" }}\n\t\"1\" {{ \"path\" \"{}\" }}\n}}",
            primary.display(),
            alt.display(),
        );
        write_file(&primary.join("steamapps/libraryfolders.vdf"), &vdf);

        let probe = Probe {
            home: home.into(),
            documents: None,
            onedrive: None,
            wineprefix: None,
            user: None,
        };
        let mut got = collect(&probe, Platform::Linux);
        rank_and_dedupe(&mut got);
        assert!(got
            .iter()
            .any(|c| c.path == alt_addons && c.source == AddonPathSource::SteamLibrary));
    }

    #[test]
    fn collect_uses_wineprefix_with_user() {
        let tmp = TempDir::new().unwrap();
        let prefix = tmp.path().join("prefix");
        let addons = prefix
            .join("drive_c/users/raggi/My Documents")
            .join(ESO_DIR)
            .join(LIVE_DIR)
            .join(ADDONS_DIR);
        touch_dir(&addons);

        let probe = Probe {
            home: tmp.path().into(),
            documents: None,
            onedrive: None,
            wineprefix: Some(prefix),
            user: Some("raggi".into()),
        };
        let mut got = collect(&probe, Platform::Linux);
        rank_and_dedupe(&mut got);
        assert!(got
            .iter()
            .any(|c| c.path == addons && c.source == AddonPathSource::Wineprefix));
    }

    #[test]
    fn collect_windows_includes_onedrive() {
        let tmp = TempDir::new().unwrap();
        let docs = tmp.path().join("Documents");
        let od = tmp.path().join("OneDrive");
        let od_docs = od.join("Documents");
        touch_dir(&eso_under_documents(&docs));
        touch_dir(&eso_under_documents(&od_docs));

        let probe = Probe {
            home: tmp.path().into(),
            documents: Some(docs.clone()),
            onedrive: Some(od.clone()),
            wineprefix: None,
            user: None,
        };
        let mut got = collect(&probe, Platform::Windows);
        rank_and_dedupe(&mut got);
        let sources: Vec<_> = got.iter().map(|c| c.source).collect();
        assert!(sources.contains(&AddonPathSource::NativeDocuments));
        assert!(sources.contains(&AddonPathSource::OneDriveDocuments));
    }

    #[test]
    fn collect_macos_native_documents() {
        let tmp = TempDir::new().unwrap();
        let docs = tmp.path().join("Documents");
        let addons = eso_under_documents(&docs);
        touch_dir(&addons);

        let probe = Probe {
            home: tmp.path().into(),
            documents: Some(docs),
            onedrive: None,
            wineprefix: None,
            user: None,
        };
        let mut got = collect(&probe, Platform::Macos);
        rank_and_dedupe(&mut got);
        assert!(got
            .iter()
            .any(|c| c.path == addons && c.source == AddonPathSource::NativeDocuments));
    }

    #[test]
    fn rank_puts_addons_exists_before_lower_status() {
        let tmp = TempDir::new().unwrap();
        let docs1 = tmp.path().join("a").join(ESO_DIR);
        touch_dir(&docs1);
        let path_lower = docs1.join(LIVE_DIR).join(ADDONS_DIR);

        let docs2 = tmp.path().join("b").join(ESO_DIR).join(LIVE_DIR).join(ADDONS_DIR);
        touch_dir(&docs2);

        let mut v = vec![
            AddonPathCandidate {
                path: path_lower.clone(),
                source: AddonPathSource::NativeDocuments,
                status: AddonPathStatus::GameSavedataExists,
            },
            AddonPathCandidate {
                path: docs2.clone(),
                source: AddonPathSource::Wineprefix,
                status: AddonPathStatus::AddOnsExists,
            },
        ];
        rank_and_dedupe(&mut v);
        assert_eq!(v[0].path, docs2);
        assert_eq!(v[1].path, path_lower);
    }
}
