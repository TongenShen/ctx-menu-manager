//! 右键菜单项的数据模型与注册表扫描。
//!
//! 一个右键菜单项在注册表里有两类形态：
//!
//! 1. **Shell 扩展（COM）** —— `...\shellex\ContextMenuHandlers\<name>`，默认值是 CLSID，
//!    真正的实现在 `HKCR\CLSID\<clsid>\InprocServer32` 指向的 DLL 里。
//! 2. **Verb 菜单项** —— `...\shell\<name>`，默认值是显示文字，
//!    命令在 `...\shell\<name>\command` 的默认值里。
//!
//! 两者都可以用同一个 `Blocked` 值临时禁用（Windows 官方认可的开关），
//! 也都可以整键删除。这就是 UI 上「临时开关」和「彻底删除」两种操作。

use std::path::Path;

use serde::{Deserialize, Serialize};
use winreg::enums::*;
use winreg::RegKey;

/// 菜单项出现在哪些位置。对应不同的注册表根路径。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// 所有文件
    File,
    /// 文件夹
    Directory,
    /// 文件夹背景（在文件夹空白处右键）
    DirectoryBackground,
    /// 桌面背景
    DesktopBackground,
    /// 驱动器
    Drive,
}

impl Scope {
    /// 该 scope 对应的、相对 `Software\Classes` 的路径。
    pub fn classes_path(self) -> &'static str {
        match self {
            Scope::File => "*",
            Scope::Directory => "Directory",
            Scope::DirectoryBackground => "Directory\\Background",
            Scope::DesktopBackground => "DesktopBackground",
            Scope::Drive => "Drive",
        }
    }

    pub fn all() -> [Scope; 5] {
        [
            Scope::File,
            Scope::Directory,
            Scope::DirectoryBackground,
            Scope::DesktopBackground,
            Scope::Drive,
        ]
    }

    /// 稳定标识，前端用来跨刷新保持选中状态。
    pub fn key(self) -> &'static str {
        match self {
            Scope::File => "file",
            Scope::Directory => "directory",
            Scope::DirectoryBackground => "directory_background",
            Scope::DesktopBackground => "desktop_background",
            Scope::Drive => "drive",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Scope::File => "文件",
            Scope::Directory => "文件夹",
            Scope::DirectoryBackground => "文件夹空白处",
            Scope::DesktopBackground => "桌面",
            Scope::Drive => "驱动器",
        }
    }

    pub fn from_key(k: &str) -> Option<Scope> {
        Scope::all().into_iter().find(|s| s.key() == k)
    }
}

/// 注册表配置单元。HKCU 优先于 HKLM（Windows 解析时也是这个顺序）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Hive {
    CurrentUser,
    LocalMachine,
}

impl Hive {
    pub fn root(self) -> RegKey {
        match self {
            Hive::CurrentUser => RegKey::predef(HKEY_CURRENT_USER),
            Hive::LocalMachine => RegKey::predef(HKEY_LOCAL_MACHINE),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Hive::CurrentUser => "HKCU",
            Hive::LocalMachine => "HKLM",
        }
    }

    pub fn from_label(s: &str) -> Option<Hive> {
        match s {
            "HKCU" => Some(Hive::CurrentUser),
            "HKLM" => Some(Hive::LocalMachine),
            _ => None,
        }
    }

    /// 该项是否需要管理员权限才能修改。
    pub fn needs_admin(self) -> bool {
        matches!(self, Hive::LocalMachine)
    }
}

/// 菜单项类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    /// COM shell 扩展
    ShellExtension,
    /// 普通 verb（命令行）
    Verb,
}

/// 一个右键菜单项的完整描述。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MenuItem {
    /// 稳定标识：`<hive>|<scope>|<kind>|<name>`，前端靠它定位。
    pub id: String,
    /// 注册表里的键名。
    pub name: String,
    /// 显示名称（尽量取友好的，取不到就用键名）。
    pub display_name: String,
    pub kind: ItemKind,
    pub scope: Scope,
    pub hive: Hive,
    /// COM 扩展的 CLSID，verb 为 None。
    pub clsid: Option<String>,
    /// 实际执行的文件（DLL 或 exe），可能为空。
    pub target: Option<String>,
    /// verb 的命令行。
    pub command: Option<String>,
    /// 是否已被 Blocked 标记禁用。
    pub blocked: bool,
    /// 修改是否需要管理员权限。
    pub needs_admin: bool,
    /// 是否是 Windows 系统自带的项（只读，不提供删除）。
    pub is_system: bool,
}

impl MenuItem {
    pub fn build_id(hive: Hive, scope: Scope, kind: ItemKind, name: &str) -> String {
        let k = match kind {
            ItemKind::ShellExtension => "ext",
            ItemKind::Verb => "verb",
        };
        format!("{}|{}|{}|{}", hive.label(), scope.key(), k, name)
    }

    /// 该菜单项在 `Software\Classes` 下的父键路径（不含 hive）。
    pub fn parent_path(&self) -> String {
        let sub = match self.kind {
            ItemKind::ShellExtension => "shellex\\ContextMenuHandlers",
            ItemKind::Verb => "shell",
        };
        format!("Software\\Classes\\{}\\{}", self.scope.classes_path(), sub)
    }

    /// 该菜单项自身的完整路径。
    pub fn full_path(&self) -> String {
        format!("{}\\{}", self.parent_path(), self.name)
    }
}

/// 从注册表读一个值的字符串形式，不存在则返回 None。
fn read_value(key: &RegKey, name: &str) -> Option<String> {
    key.get_value::<String, _>(name).ok()
}

/// 读默认值（REG_SZ）。
fn read_default(key: &RegKey) -> Option<String> {
    key.get_value::<String, _>("").ok()
}

/// 在 HKCU / HKLM 中查找 CLSID 的注册信息，返回 (友好名, 服务器路径)。
fn resolve_clsid(clsid: &str) -> (Option<String>, Option<String>) {
    for hive in [Hive::CurrentUser, Hive::LocalMachine] {
        let path = format!("Software\\Classes\\CLSID\\{}", clsid);
        if let Ok(key) = hive.root().open_subkey(&path) {
            let friendly = read_default(&key);

            // InprocServer32 是 DLL；LocalServer32 是 exe。
            let mut server = None;
            for sub in ["InprocServer32", "LocalServer32"] {
                if let Ok(sk) = key.open_subkey(sub) {
                    if let Some(v) = read_default(&sk) {
                        if !v.trim().is_empty() {
                            server = Some(clean_path(&v));
                            break;
                        }
                    }
                }
            }
            return (friendly, server);
        }
    }
    (None, None)
}

/// 去掉路径里的环境变量和引号，取文件名便于展示。
fn clean_path(raw: &str) -> String {
    let trimmed = raw.trim().trim_matches('"').trim();
    // 展开常见环境变量前缀，便于判断是否系统目录。
    let expanded = trimmed
        .replace("%SystemRoot%", "C:\\Windows")
        .replace("%windir%", "C:\\Windows")
        .replace("%SystemDrive%", "C:");
    expanded
}

/// 判断一个服务器路径是否指向 Windows 系统目录。
fn looks_system(target: &str) -> bool {
    let lower = target.to_ascii_lowercase();
    lower.contains("\\windows\\system32\\") || lower.contains("\\windows\\syswow64\\")
}

/// 扫描某个 hive + scope 下的所有菜单项。
pub fn scan_scope(hive: Hive, scope: Scope, out: &mut Vec<MenuItem>) {
    let root = hive.root();

    // ---- 1. Shell 扩展 ----
    let ext_path = format!(
        "Software\\Classes\\{}\\shellex\\ContextMenuHandlers",
        scope.classes_path()
    );
    if let Ok(ext_key) = root.open_subkey(&ext_path) {
        for name in ext_key.enum_keys().flatten() {
            let Ok(sub) = ext_key.open_subkey(&name) else {
                continue;
            };
            let clsid = read_default(&sub);
            let blocked = sub.get_value::<u32, _>("Blocked").is_ok()
                || read_value(&sub, "Blocked").is_some();

            let (friendly, server) = match &clsid {
                Some(c) => resolve_clsid(c),
                None => (None, None),
            };

            // 显示名优先用 CLSID 的友好名，其次用键名。
            let display = friendly
                .as_ref()
                .filter(|s| !s.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| name.clone());

            let is_system = server.as_deref().map(looks_system).unwrap_or(false)
                || matches!(name.as_str(), "Open With" | "Sharing" | "WorkFolders");

            out.push(MenuItem {
                id: MenuItem::build_id(hive, scope, ItemKind::ShellExtension, &name),
                name: name.clone(),
                display_name: display,
                kind: ItemKind::ShellExtension,
                scope,
                hive,
                clsid,
                target: server,
                command: None,
                blocked,
                needs_admin: hive.needs_admin(),
                is_system,
            });
        }
    }

    // ---- 2. Verb 菜单项 ----
    let verb_path = format!("Software\\Classes\\{}\\shell", scope.classes_path());
    if let Ok(verb_key) = root.open_subkey(&verb_path) {
        for name in verb_key.enum_keys().flatten() {
            let Ok(sub) = verb_key.open_subkey(&name) else {
                continue;
            };
            let blocked = sub.get_value::<u32, _>("Blocked").is_ok()
                || read_value(&sub, "Blocked").is_some();

            // verb 的默认值可能是显示文字，也可能是 MUIVerb 指示的本地化串。
            let mut display = read_default(&sub).unwrap_or_default();
            if display.is_empty() {
                if let Some(mui) = read_value(&sub, "MUIVerb") {
                    display = mui;
                }
            }
            if display.is_empty() {
                display = name.clone();
            }
            // 形如 "@shell32.dll,-8506" 的本地化引用，保留原样但去掉前缀便于阅读。
            if display.starts_with('@') {
                display = format!("[本地化] {}", name);
            }

            // 命令：优先取 command 子键的默认值。
            let command = sub
                .open_subkey("command")
                .ok()
                .and_then(|c| read_default(&c));

            // 只有一个默认子键的容器（如 "Open With"）不是真正的命令项。
            let has_command = command.is_some();

            let is_system = command
                .as_deref()
                .map(|c| {
                    let l = c.to_ascii_lowercase();
                    l.contains("\\windows\\system32\\") || l.contains("\\windows\\syswow64\\")
                })
                .unwrap_or(false);

            out.push(MenuItem {
                id: MenuItem::build_id(hive, scope, ItemKind::Verb, &name),
                name: name.clone(),
                display_name: display,
                kind: ItemKind::Verb,
                scope,
                hive,
                clsid: None,
                target: command.as_deref().map(clean_path),
                command,
                blocked,
                needs_admin: hive.needs_admin(),
                is_system: is_system || !has_command,
            });
        }
    }
}

/// 扫描所有位置的所有菜单项。
pub fn scan_all() -> Vec<MenuItem> {
    let mut items = Vec::new();
    for hive in [Hive::CurrentUser, Hive::LocalMachine] {
        for scope in Scope::all() {
            scan_scope(hive, scope, &mut items);
        }
    }
    items.sort_by(|a, b| {
        (a.scope.key(), a.hive.label(), &a.display_name)
            .cmp(&(b.scope.key(), b.hive.label(), &b.display_name))
    });
    items
}

/// 判断文件是否存在，用于标记「指向的文件已丢失」。
pub fn target_exists(target: &str) -> bool {
    Path::new(target).exists()
}
