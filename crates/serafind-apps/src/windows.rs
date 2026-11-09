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

/// Store (UWP) apps: the items of the Start menu's own list (the `shell:AppsFolder` virtual
/// folder) whose id has a `!`. Started through `shell:AppsFolder\<id>`. Read through the Shell
/// API, without starting any program.
fn store_apps() -> Vec<App> {
    let items = match apps_folder::items() {
        Ok(items) => items,
        Err(error) => {
            eprintln!("serafind: cannot list the Store apps: {error}");
            return Vec::new();
        }
    };
    items
        .into_iter()
        .filter(|(_, id)| id.contains('!'))
        .map(|(name, id)| {
            let target = PathBuf::from(format!(r"shell:AppsFolder\{id}"));
            App {
                id,
                name,
                source: target.clone(),
                executable: None,
                icon: None,
                launch: Launch::Open(target),
            }
        })
        .collect()
}

/// The `shell:AppsFolder` virtual folder, through COM.
mod apps_folder {
    use windows::Win32::System::Com::{
        COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize,
    };
    use windows::Win32::UI::Shell::{
        BHID_EnumItems, FOLDERID_AppsFolder, IEnumShellItems, IShellItem, KF_FLAG_DEFAULT,
        SHGetKnownFolderItem, SIGDN, SIGDN_NORMALDISPLAY, SIGDN_PARENTRELATIVEPARSING,
    };
    use windows::core::Result;

    /// The name shown and the id (`AppUserModelID`) of every item.
    pub fn items() -> Result<Vec<(String, String)>> {
        // SAFETY: COM is set up on this thread for the time of the call and released after
        // (only when this call set it up); the interfaces are reference counted by the windows
        // crate; each string the Shell allocates is copied, then freed with CoTaskMemFree.
        #[allow(unsafe_code)]
        unsafe {
            let set_up = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
            let result = (|| {
                let folder: IShellItem = SHGetKnownFolderItem(&FOLDERID_AppsFolder, KF_FLAG_DEFAULT, None)?;
                let list: IEnumShellItems = folder.BindToHandler(None, &BHID_EnumItems)?;
                let mut items = Vec::new();
                loop {
                    let mut next = [None];
                    let mut fetched = 0u32;
                    list.Next(&mut next, Some(&raw mut fetched))?;
                    let Some(item) = next[0].take().filter(|_| fetched == 1) else {
                        break;
                    };
                    let (Ok(name), Ok(id)) = (
                        text(&item, SIGDN_NORMALDISPLAY),
                        text(&item, SIGDN_PARENTRELATIVEPARSING),
                    ) else {
                        continue;
                    };
                    items.push((name, id));
                }
                Ok(items)
            })();
            if set_up {
                CoUninitialize();
            }
            result
        }
    }

    /// A display name of an item, as a Rust string.
    #[allow(unsafe_code)]
    unsafe fn text(item: &IShellItem, kind: SIGDN) -> Result<String> {
        // SAFETY: the Shell returns a string it allocated: read it, then free it.
        unsafe {
            let raw = item.GetDisplayName(kind)?;
            let text = raw.to_string().unwrap_or_default();
            CoTaskMemFree(Some(raw.0.cast_const().cast()));
            Ok(text)
        }
    }
}
