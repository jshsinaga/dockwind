use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use crate::config::PinnedApp;

#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub hwnd: isize,
    pub title: String,
    pub exe_path: Option<PathBuf>,
    pub process_id: u32,
    pub display_name: Option<String>,
    pub is_minimized: bool,
    pub is_foreground: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockItem {
    pub key: PathBuf,
    has_executable: bool,
    pub name: String,
    pub args: Vec<String>,
    pub windows: Vec<isize>,
    pub active_window: Option<isize>,
    pub is_pinned: bool,
    pub is_running: bool,
    pub is_active: bool,
}

impl DockItem {
    pub fn executable_path(&self) -> Option<&Path> {
        self.has_executable.then_some(self.key.as_path())
    }

    pub fn target_window(&self) -> Option<isize> {
        self.active_window.or_else(|| self.windows.first().copied())
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
enum WindowGroup {
    Executable(String),
    Process(u32),
}

pub fn merge(
    pinned: &[PinnedApp],
    running: &[WindowInfo],
    show_running_apps: bool,
) -> Vec<DockItem> {
    let mut windows_by_app: BTreeMap<WindowGroup, Vec<&WindowInfo>> = BTreeMap::new();
    for window in running {
        let group = window
            .exe_path
            .as_deref()
            .map_or(WindowGroup::Process(window.process_id), |path| {
                WindowGroup::Executable(normalize_path(path))
            });
        windows_by_app.entry(group).or_default().push(window);
    }

    let mut pinned_keys = HashSet::with_capacity(pinned.len());
    let mut items = Vec::with_capacity(pinned.len());

    for app in pinned {
        let key = WindowGroup::Executable(normalize_path(Path::new(&app.path)));
        let windows = windows_by_app.get(&key).map_or(&[][..], Vec::as_slice);
        let (target, is_active) = window_target(windows);
        pinned_keys.insert(key);

        items.push(DockItem {
            key: PathBuf::from(&app.path),
            has_executable: true,
            name: app.name.clone(),
            args: app.args.clone(),
            windows: windows.iter().map(|window| window.hwnd).collect(),
            active_window: target,
            is_pinned: true,
            is_running: !windows.is_empty(),
            is_active,
        });
    }

    if !show_running_apps {
        return items;
    }

    for (key, windows) in windows_by_app {
        if pinned_keys.contains(&key) {
            continue;
        }
        let first = windows[0];
        let path = first.exe_path.as_deref();
        let (target, is_active) = window_target(&windows);
        let name = windows
            .iter()
            .find_map(|window| window.display_name.as_deref())
            .map(ToString::to_string)
            .or_else(|| {
                path.and_then(Path::file_stem)
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .filter(|stem| !stem.is_empty())
            })
            .or_else(|| {
                windows
                    .iter()
                    .find(|window| !window.title.trim().is_empty())
                    .map(|window| window.title.clone())
            })
            .unwrap_or_else(|| format!("App {}", first.process_id));

        items.push(DockItem {
            key: path.map_or_else(
                || PathBuf::from(format!("process:{}", first.process_id)),
                Path::to_path_buf,
            ),
            has_executable: path.is_some(),
            name,
            args: Vec::new(),
            windows: windows.iter().map(|window| window.hwnd).collect(),
            active_window: target,
            is_pinned: false,
            is_running: true,
            is_active,
        });
    }

    items
}

fn window_target(windows: &[&WindowInfo]) -> (Option<isize>, bool) {
    if let Some(foreground) = windows.iter().find(|window| window.is_foreground) {
        return (Some(foreground.hwnd), true);
    }
    let target = windows
        .iter()
        .find(|window| !window.is_minimized)
        .or_else(|| windows.first())
        .map(|window| window.hwnd);
    (target, false)
}

fn normalize_path(path: &Path) -> String {
    let normalized = path
        .to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase();
    if let Some(unc) = normalized.strip_prefix(r"\\?\unc\") {
        format!(r"\\{unc}")
    } else if let Some(local) = normalized.strip_prefix(r"\\?\") {
        local.to_owned()
    } else {
        normalized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(hwnd: isize, path: &str, foreground: bool) -> WindowInfo {
        WindowInfo {
            hwnd,
            title: "window".to_string(),
            exe_path: Some(PathBuf::from(path)),
            process_id: hwnd as u32,
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
    fn inaccessible_executable_still_has_an_activatable_running_tile() {
        let mut first = window(10, "unused.exe", false);
        first.exe_path = None;
        first.title = "Elevated editor".into();
        let mut second = first.clone();
        second.hwnd = 11;
        second.is_foreground = true;
        let items = merge(&[], &[first, second], true);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "Elevated editor");
        assert_eq!(items[0].target_window(), Some(11));
        assert!(items[0].executable_path().is_none());
    }

    #[test]
    fn extended_path_and_slash_variants_match_the_same_pin() {
        let pinned = [pinned("Editor", "C:/Apps/Editor.exe")];
        let running = [window(1, r"\\?\C:\Apps\editor.exe", true)];
        let items = merge(&pinned, &running, true);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].target_window(), Some(1));
    }

    #[test]
    fn pinned_apps_keep_config_order_and_merge_running_windows() {
        let pinned = vec![
            pinned("Terminal", r"C:\Windows\System32\cmd.exe"),
            pinned("Explorer", r"C:\Windows\explorer.exe"),
        ];
        let running = vec![
            window(1, "C:/Windows/System32/notepad.exe", true),
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
    fn duplicate_pinned_paths_keep_both_entries_and_share_the_window_group() {
        let pinned = [
            pinned("Editor", "C:/Apps/Editor.exe"),
            pinned("Editor profile", "c:/apps/editor.exe"),
        ];
        let mut minimized = window(1, "C:/Apps/Editor.exe", false);
        minimized.is_minimized = true;
        let running = [minimized, window(2, "C:/Apps/Editor.exe", false)];

        let items = merge(&pinned, &running, true);
        assert_eq!(items.len(), 2);
        assert!(items.iter().all(|item| item.target_window() == Some(2)));
        assert!(items.iter().all(|item| item.windows == [1, 2]));
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
