//! 右键菜单项的数据模型与注册表扫描。
//!
//! 一个右键菜单项在注册表里有两类形态：
//!
//! 1. **Shell 扩展（COM）** —— `...\shellex\ContextMenuHandlers\<name>`，默认值是 CLSID，
//!    真正的实现在 `HKCR\CLSID\<clsid>\InprocServer32` 指向的 DLL 里。
//! 2. **Verb 菜单项** —— `...\shell\<name>`，默认值是显示文字，
//!    命令在 `...\shell\<name>\command` 的默认值里。
//!
//! **这两类的「临时禁用」机制完全不同，不能混用**（这是常见的踩坑点）：
//!
//! - Shell 扩展：把 **CLSID** 写进
//!   `HKCU\Software\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked`
//!   这个列表（键名是 CLSID，值是随便一个字符串，通常空串）。在扩展自身键下
//!   加 `Blocked` 值**没有任何效果**。
//! - Verb：在 verb 键下加 `LegacyDisable`。这个值只需要存在，类型和数据都不重要
//!   （微软文档：[Verbs and File Associations]）。在 verb 下加 `Blocked` 同样无效。
//!
//! 两类都可以整键删除，那就是 UI 上的「删除」。

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
    /// 是否已被禁用（扩展看屏蔽列表，verb 看 LegacyDisable）。
    pub blocked: bool,
    /// **删除/改键**是否需要管理员权限（取决于该项本身在 HKCU 还是 HKLM）。
    pub needs_admin: bool,
    /// **临时禁用**是否需要管理员权限。
    ///
    /// 与 `needs_admin` 不同：shell 扩展的禁用是往 HKCU 的屏蔽列表里加 CLSID，
    /// 即使扩展本身注册在 HKLM 下也不需要提权；verb 的禁用要改它自己的键，
    /// 所以 HKLM 下的 verb 才需要管理员。
    pub toggle_needs_admin: bool,
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

/// Shell 扩展的全局屏蔽列表（按 CLSID）。
///
/// 这是 Windows 官方认可的 shell 扩展开关：键名是 CLSID，值内容无所谓。
/// HKCU 影响当前用户，HKLM 影响所有用户。
pub const EXT_BLOCKLIST: &str =
    "Software\\Microsoft\\Windows\\CurrentVersion\\Shell Extensions\\Blocked";

/// 某个 CLSID 是否在屏蔽列表里。
pub fn is_clsid_blocked(clsid: &str) -> bool {
    for hive in [Hive::CurrentUser, Hive::LocalMachine] {
        if let Ok(key) = hive.root().open_subkey(EXT_BLOCKLIST) {
            if key.get_value::<String, _>(clsid).is_ok() {
                return true;
            }
        }
    }
    false
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
                            server = Some(normalize_path(&v));
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
/// 处理 `InprocServer32` / `LocalServer32` 的值：这是一个**纯文件路径**，
/// 即使含空格也不带引号，所以整条保留，只展开环境变量。
fn normalize_path(raw: &str) -> String {
    raw.trim()
        .trim_matches('"')
        .trim()
        .replace("%SystemRoot%", "C:\\Windows")
        .replace("%windir%", "C:\\Windows")
        .replace("%SystemDrive%", "C:")
}

/// 从**命令行**里取出可执行文件路径，用于列表展示。
///
/// 命令行和纯路径不同，形态有好几种：
///   `"C:\Program Files\App\app.exe" "%1"`
///   `C:\Windows\notepad.exe %1`
///   `%SystemRoot%\system32\cmd.exe /c ...`
/// 展示时只需要最前面的可执行文件，参数一律丢掉。
fn clean_command(raw: &str) -> String {
    let s = raw.trim();

    // 带引号：取第一对引号之间的内容（这样路径里的空格不会误伤）。
    let path = if let Some(stripped) = s.strip_prefix('"') {
        match stripped.find('"') {
            Some(end) => &stripped[..end],
            None => stripped,
        }
    } else {
        // 不带引号：按空格切，但只切一次，取第一段。
        match s.find(' ') {
            Some(end) => &s[..end],
            None => s,
        }
    };

    path.trim()
        .replace("%SystemRoot%", "C:\\Windows")
        .replace("%windir%", "C:\\Windows")
        .replace("%SystemDrive%", "C:")
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
            // 扩展的禁用状态看的是全局屏蔽列表里有没有这个 CLSID，
            // 不是扩展键自己下面的某个值。
            let blocked = clsid
                .as_deref()
                .map(is_clsid_blocked)
                .unwrap_or(false);

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
                // 扩展禁用走 HKCU 屏蔽列表，永远不需要提权。
                toggle_needs_admin: false,
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
            // Verb 的禁用标记是 LegacyDisable：只需要存在，类型无所谓。
            let blocked = sub.get_value::<String, _>("LegacyDisable").is_ok()
                || sub.get_raw_value("LegacyDisable").is_ok();

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
                target: command.as_deref().map(clean_command),
                command,
                blocked,
                needs_admin: hive.needs_admin(),
                // verb 的禁用要改自己的键，HKLM 下的才需要提权。
                toggle_needs_admin: hive.needs_admin(),
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
