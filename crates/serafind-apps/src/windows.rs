//! Applications on Windows: the shortcuts of the Start menu, and the Microsoft Store apps.

use std::path::{Path, PathBuf};

use crate::{App, Launch};

/// The Start menu folders: for all users, then for this user.
#[must_use]
pub fn application_folders() -> Vec<PathBuf> {
    let mut folders = Vec::new();
    if let Some(program_data) = std::env::var_os("ProgramData") {
        folders.push(PathBuf::from(program_data).join(r"Microsoft\Windows\Start Menu\Programs"));
    }
    if let Some(app_data) = dirs::data_dir() {
        folders.push(app_data.join(r"Microsoft\Windows\Start Menu\Programs"));
    }
    folders
}

/// Every application: Start menu shortcuts, then Store apps.
#[must_use]
pub fn discover(folders: &[PathBuf]) -> Vec<App> {
    let mut apps = Vec::new();
    for folder in folders {
        shortcuts(folder, folder, &mut apps);
    }
    apps.extend(store_apps());
    apps
}

fn shortcuts(root: &Path, folder: &Path, apps: &mut Vec<App>) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            shortcuts(root, &path, apps);
            continue;
        }
        if !path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("lnk"))
        {
            continue;
        }
        let Some(name) = path.file_stem().map(|stem| stem.to_string_lossy().into_owned()) else {
            continue;
        };
        // Uninstallers clutter the Start menu folders.
        if name.to_lowercase().starts_with("uninstall") {
            continue;
        }
        let id = path.strip_prefix(root).map_or_else(
            |_| path.to_string_lossy().into_owned(),
            |relative| relative.to_string_lossy().into_owned(),
        );
        if apps.iter().any(|app| app.id == id) {
            continue;
        }
        apps.push(App {
            id,
            name,
            source: path.clone(),
            executable: None,
            icon: None,
            launch: Launch::Open(path),
        });
    }
}

#[derive(serde::Deserialize)]
struct StartApp {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "AppID")]
    app_id: String,
}

/// Store (UWP) apps, from `Get-StartApps`: their id has a `!`. Started through
/// `shell:AppsFolder`.
fn store_apps() -> Vec<App> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let output = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-StartApps | ConvertTo-Json -Compress",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let list: Vec<StartApp> = serde_json::from_str::<Vec<StartApp>>(&text)
        .or_else(|_| serde_json::from_str::<StartApp>(&text).map(|app| vec![app]))
        .unwrap_or_default();
    list.into_iter()
        .filter(|app| app.app_id.contains('!'))
        .map(|app| {
            let target = PathBuf::from(format!(r"shell:AppsFolder\{}", app.app_id));
            App {
                id: app.app_id,
                name: app.name,
                source: target.clone(),
                executable: None,
                icon: None,
                launch: Launch::Open(target),
            }
        })
        .collect()
}
