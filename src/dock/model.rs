use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use crate::config::PinnedApp;

#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub hwnd: isize,
    pub title: String,
    pub exe_path: PathBuf,
    pub display_name: Option<String>,
    pub is_minimized: bool,
    pub is_foreground: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockItem {
    pub key: PathBuf,
    pub name: String,
    pub args: Vec<String>,
    pub windows: Vec<isize>,
    pub active_window: Option<isize>,
    pub is_pinned: bool,
    pub is_running: bool,
    pub is_active: bool,
}

impl DockItem {
    pub fn target_window(&self) -> Option<isize> {
        self.active_window.or_else(|| self.windows.first().copied())
    }
}

pub fn merge(
    pinned: &[PinnedApp],
    running: &[WindowInfo],
    show_running_apps: bool,
) -> Vec<DockItem> {
    let mut pinned_keys = HashSet::new();
    let mut items = Vec::new();

    for app in pinned {
        let key = normalize_path(Path::new(&app.path));
        pinned_keys.insert(key.clone());
        let windows = running
            .iter()
            .filter(|window| normalize_path(&window.exe_path) == key)
            .collect::<Vec<_>>();
        let active = windows
            .iter()
            .find(|window| window.is_foreground)
            .map(|window| window.hwnd);
        let target = active.or_else(|| {
            windows
                .iter()
                .find(|window| !window.is_minimized)
                .or_else(|| windows.first())
                .map(|window| window.hwnd)
        });

        items.push(DockItem {
            key: PathBuf::from(&app.path),
            name: app.name.clone(),
            args: app.args.clone(),
            windows: windows.iter().map(|window| window.hwnd).collect(),
            active_window: target,
            is_pinned: true,
            is_running: !windows.is_empty(),
            is_active: active.is_some(),
        });
    }

    if !show_running_apps {
        return items;
    }

    let mut remaining: BTreeMap<String, (PathBuf, Vec<&WindowInfo>)> = BTreeMap::new();
    for window in running {
        let key = normalize_path(&window.exe_path);
        if pinned_keys.contains(&key) {
            continue;
        }
        remaining
            .entry(key)
            .or_insert_with(|| (window.exe_path.clone(), Vec::new()))
            .1
            .push(window);
    }

    for (_, (path, windows)) in remaining {
        let active = windows
            .iter()
            .find(|window| window.is_foreground)
            .map(|window| window.hwnd);
        let target = active.or_else(|| {
            windows
                .iter()
                .find(|window| !window.is_minimized)
                .or_else(|| windows.first())
                .map(|window| window.hwnd)
        });
        let name = windows
            .iter()
            .find_map(|window| window.display_name.as_deref())
            .map(ToString::to_string)
            .or_else(|| {
                path.file_stem()
                    .map(|stem| stem.to_string_lossy().to_string())
                    .filter(|stem| !stem.is_empty())
            })
            .or_else(|| windows.first().map(|window| window.title.clone()))
            .unwrap_or_else(|| path.to_string_lossy().to_string());

        items.push(DockItem {
            key: path,
            name,
            args: Vec::new(),
            windows: windows.iter().map(|window| window.hwnd).collect(),
            active_window: target,
            is_pinned: false,
            is_running: true,
            is_active: active.is_some(),
        });
    }

    items
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(hwnd: isize, path: &str, foreground: bool) -> WindowInfo {
        WindowInfo {
            hwnd,
            title: "window".to_string(),
            exe_path: PathBuf::from(path),
            display_name: None,
            is_minimized: false,
            is_foreground: foreground,
        }
    }

    fn pinned(name: &str, path: &str) -> PinnedApp {
        PinnedApp {
            name: name.to_string(),
            path: path.to_string(),
            args: Vec::new(),
        }
    }

    #[test]
    fn pinned_apps_keep_config_order_and_merge_running_windows() {
        let pinned = vec![
            pinned("Terminal", r"C:\Windows\System32\cmd.exe"),
            pinned("Explorer", r"C:\Windows\explorer.exe"),
        ];
        let running = vec![
            window(1, r"C:\Windows\System32\notepad.exe", true),
            window(2, r"C:\Windows\System32\cmd.exe", false),
        ];
        let items = merge(&pinned, &running, true);

        assert_eq!(items.len(), 3);
        assert_eq!(items[0].name, "Terminal");
        assert!(items[0].is_pinned);
        assert!(items[0].is_running);
        assert_eq!(items[0].windows, vec![2]);
        assert_eq!(items[1].name, "Explorer");
        assert!(!items[1].is_running);
        assert_eq!(items[2].name, "notepad");
        assert!(items[2].is_active);
        assert_eq!(items[2].target_window(), Some(1));
    }

    #[test]
    fn running_windows_with_the_same_exe_are_grouped() {
        let running = vec![
            window(10, r"C:\Apps\Editor.exe", false),
            window(11, r"C:\Apps\editor.exe", true),
        ];
        let items = merge(&[], &running, true);

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].windows, vec![10, 11]);
        assert_eq!(items[0].active_window, Some(11));
        assert!(items[0].is_active);
    }

    #[test]
    fn path_comparison_is_case_insensitive() {
        let pinned = vec![pinned("Terminal", r"C:\WINDOWS\System32\CMD.EXE")];
        let running = vec![window(5, r"c:\windows\system32\cmd.exe", false)];
        let items = merge(&pinned, &running, true);

        assert_eq!(items.len(), 1);
        assert!(items[0].is_running);
    }

    #[test]
    fn display_name_overrides_the_executable_stem() {
        let mut uwp = window(
            1,
            r"C:\Windows\ImmersiveControlPanel\SystemSettings.exe",
            false,
        );
        uwp.display_name = Some("Settings".to_string());
        let items = merge(&[], &[uwp], true);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "Settings");
        assert_eq!(
            items[0].key,
            PathBuf::from(r"C:\Windows\ImmersiveControlPanel\SystemSettings.exe")
        );
    }

    #[test]
    fn running_only_apps_can_be_hidden() {
        let pinned = vec![pinned("Terminal", r"C:\Windows\System32\cmd.exe")];
        let running = vec![
            window(1, r"C:\Windows\System32\notepad.exe", true),
            window(2, r"C:\Windows\System32\cmd.exe", false),
        ];
        let items = merge(&pinned, &running, false);

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "Terminal");
        assert!(items[0].is_running);
    }
}
