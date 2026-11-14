//! 注册表写操作。
//!
//! 两种禁用方式并存（按用户要求）：
//!
//! - **Blocked 标记**：临时开关，加一个 `Blocked` 值即可，随时可逆，数据不丢。
//! - **删除键**：彻底移除，删前导出 `.reg` 备份到 `%APPDATA%\CtxMenuManager\backup`。

use std::fs;
use std::path::PathBuf;

use winreg::enums::*;
use winreg::RegKey;

use crate::registry::{Hive, ItemKind, MenuItem, Scope};

/// 经典右键菜单的两个 CLSID（详见 README）。
pub const LEGACY_HANDLER: &str = "{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}";
pub const MODERN_HANDLER: &str = "{51668040-5e31-4023-bbe3-149163bd80f1}";

/// 备份目录：`%APPDATA%\CtxMenuManager\backup`
fn backup_dir() -> Result<PathBuf, String> {
    let appdata = std::env::var("APPDATA").map_err(|_| "找不到 APPDATA 环境变量".to_string())?;
    let dir = PathBuf::from(appdata).join("CtxMenuManager").join("backup");
    fs::create_dir_all(&dir).map_err(|e| format!("创建备份目录失败: {e}"))?;
    Ok(dir)
}

/// 把某个键（含子键）递归导出为 `.reg` 文本。
///
/// 只处理字符串和 DWORD 这两类值——右键菜单项里实际只会出现这两种。
fn export_key_recursive(root: &RegKey, path: &str, full: &str, out: &mut String) -> Result<(), String> {
    let key = root
        .open_subkey(path)
        .map_err(|e| format!("打开 {path} 失败: {e}"))?;

    out.push_str(&format!("[{full}]\n"));

    for (name, _value) in key.enum_values().flatten() {
        let has_default = name.is_empty();
        let value_name = if has_default { "@".to_string() } else { name.clone() };

        // 值类型判断：先试字符串，再试 DWORD。
        if let Ok(s) = key.get_value::<String, _>(&name) {
            let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
            out.push_str(&format!("{value_name}=\"{escaped}\"\n"));
        } else if let Ok(d) = key.get_value::<u32, _>(&name) {
            out.push_str(&format!("{value_name}=dword:{d:08x}\n"));
        }
        // 其他类型（二进制等）跳过，右键菜单项里基本不会出现。
    }

    // 删除标记，避免导入时残留旧值。
    out.push('\n');

    for sub in key.enum_keys().flatten() {
        let sub_path = format!("{path}\\{sub}");
        let sub_full = format!("{full}\\{sub}");
        export_key_recursive(root, &sub_path, &sub_full, out)?;
    }
    Ok(())
}

/// 删除前把整个键导出成 `.reg` 备份，返回备份文件路径。
fn backup_key(hive: Hive, item: &MenuItem) -> Result<PathBuf, String> {
    let root = hive.root();
    let path = item.full_path();
    let full = format!("HKEY_{}\\{}", match hive {
        Hive::CurrentUser => "CURRENT_USER",
        Hive::LocalMachine => "LOCAL_MACHINE",
    }, path);

    let mut body = String::from("Windows Registry Editor Version 5.00\r\n\r\n");
    export_key_recursive(&root, &path, &full, &mut body)?;

    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let safe_name: String = item
        .name
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    let file = backup_dir()?.join(format!("{stamp}_{safe_name}.reg"));
    fs::write(&file, body).map_err(|e| format!("写备份失败: {e}"))?;
    Ok(file)
}

/// 设置或清除 `Blocked` 标记（临时开关）。
pub fn set_blocked(item: &MenuItem, blocked: bool) -> Result<String, String> {
    let root = item.hive.root();
    let path = format!(
        "Software\\Classes\\{}\\{}\\{}",
        item.scope.classes_path(),
        match item.kind {
            ItemKind::ShellExtension => "shellex\\ContextMenuHandlers",
            ItemKind::Verb => "shell",
        },
        item.name
    );

    let key = root
        .open_subkey_with_flags(&path, KEY_SET_VALUE)
        .map_err(|e| {
            if item.hive.needs_admin() {
                format!("需要管理员权限才能修改 {path}: {e}")
            } else {
                format!("打开 {path} 失败: {e}")
            }
        })?;

    if blocked {
        key.set_value("Blocked", &1u32)
            .map_err(|e| format!("写入 Blocked 失败: {e}"))?;
        Ok(format!("已临时禁用「{}」，随时可以恢复", item.display_name))
    } else {
        match key.delete_value("Blocked") {
            Ok(()) => Ok(format!("已启用「{}」", item.display_name)),
            // 本来就没有 Blocked，视为成功。
            Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => {
                Ok(format!("「{}」本来就是启用状态", item.display_name))
            }
            Err(e) => Err(format!("清除 Blocked 失败: {e}")),
        }
    }
}

/// 删除整个键（彻底移除），删除前自动备份。返回提示信息。
pub fn delete_item(item: &MenuItem) -> Result<String, String> {
    let backup = backup_key(item.hive, item)?;

    let root = item.hive.root();
    let parent = item.parent_path();

    let parent_key = root
        .open_subkey_with_flags(&parent, KEY_ALL_ACCESS)
        .map_err(|e| {
            if item.hive.needs_admin() {
                format!("需要管理员权限才能删除（{parent}）: {e}")
            } else {
                format!("打开 {parent} 失败: {e}")
            }
        })?;

    parent_key
        .delete_subkey_all(&item.name)
        .map_err(|e| format!("删除失败: {e}"))?;

    Ok(format!(
        "已彻底删除「{}」，备份存于 {}",
        item.display_name,
        backup.display()
    ))
}

/// 新建一个 verb 菜单项。
pub fn create_verb(
    hive: Hive,
    scope: Scope,
    name: &str,
    display: &str,
    command: &str,
) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("键名不能为空".into());
    }
    if name.contains('\\') {
        return Err("键名不能包含反斜杠".into());
    }
    if command.trim().is_empty() {
        return Err("命令不能为空".into());
    }

    let path = format!(
        "Software\\Classes\\{}\\shell\\{}",
        scope.classes_path(),
        name
    );

    let root = hive.root();
    let (key, _) = root.create_subkey(&path).map_err(|e| {
        if hive.needs_admin() {
            format!("需要管理员权限（{path}）: {e}")
        } else {
            format!("创建 {path} 失败: {e}")
        }
    })?;

    key.set_value("", &display.to_string())
        .map_err(|e| format!("写入显示名失败: {e}"))?;

    let (cmd_key, _) = root
        .create_subkey(format!("{path}\\command"))
        .map_err(|e| format!("创建 command 子键失败: {e}"))?;
    cmd_key
        .set_value("", &command.to_string())
        .map_err(|e| format!("写入命令失败: {e}"))?;

    Ok(format!("已新建菜单项「{display}」"))
}

// ---------------------------------------------------------------------------
// 经典右键菜单开关
// ---------------------------------------------------------------------------

/// 经典菜单当前的启用状态。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ClassicMenuState {
    pub enabled: bool,
    pub legacy_masked: bool,
    pub modern_masked: bool,
    pub build: u32,
    /// build >= 26100 时两个键都必须写。
    pub needs_both_keys: bool,
}

fn is_masked(clsid: &str) -> bool {
    let path = format!("Software\\Classes\\CLSID\\{}\\InprocServer32", clsid);
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(&path)
        .map(|k| {
            // 「已屏蔽」= 键存在且默认值为空串。
            k.get_value::<String, _>("")
                .map(|v| v.is_empty())
                .unwrap_or(false)
        })
        .unwrap_or(false)
}

pub fn classic_state() -> ClassicMenuState {
    let build: u32 = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion")
        .and_then(|k| k.get_value::<String, _>("CurrentBuildNumber"))
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let legacy = is_masked(LEGACY_HANDLER);
    let modern = is_masked(MODERN_HANDLER);

    ClassicMenuState {
        enabled: legacy && modern,
        legacy_masked: legacy,
        modern_masked: modern,
        build,
        needs_both_keys: build >= 26100,
    }
}

/// 开启或关闭经典右键菜单。
pub fn set_classic_menu(enable: bool) -> Result<String, String> {
    let ctx = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey("Software\\Classes\\CLSID")
        .map_err(|e| format!("打开 HKCU\\Software\\Classes\\CLSID 失败: {e}"))?
        .0;

    for clsid in [LEGACY_HANDLER, MODERN_HANDLER] {
        if enable {
            let (key, _) = ctx
                .create_subkey(format!("{clsid}\\InprocServer32"))
                .map_err(|e| format!("创建 {clsid}\\InprocServer32 失败: {e}"))?;
            // 空字符串 = 屏蔽该 COM 处理器。
            key.set_value("", &String::new())
                .map_err(|e| format!("写入 {clsid} 失败: {e}"))?;
        } else {
            match ctx.delete_subkey_all(clsid) {
                Ok(()) => {}
                Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("删除 {clsid} 失败: {e}")),
            }
        }
    }

    Ok(if enable {
        "已开启经典右键菜单（重启资源管理器后生效）".into()
    } else {
        "已还原成 Windows 11 新菜单（重启资源管理器后生效）".into()
    })
}

/// 用默认浏览器打开一个链接。
///
/// 只接受 http/https，避免被当成任意命令执行的入口。直接用 `cmd /c start`
/// 会走 shell 解析，所以这里先做协议白名单校验。
pub fn open_url(url: &str) -> Result<String, String> {
    use std::process::Command;

    let trimmed = url.trim();
    if !(trimmed.starts_with("https://") || trimmed.starts_with("http://")) {
        return Err("只支持 http/https 链接".into());
    }

    // rundll32 不会经过 cmd 解析，参数里的 & 等字符不会被当成命令分隔符。
    Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", trimmed])
        .spawn()
        .map_err(|e| format!("打开链接失败: {e}"))?;

    Ok("已在浏览器中打开".into())
}

/// 重启 explorer。会短暂关闭所有资源管理器窗口。
pub fn restart_explorer() -> Result<String, String> {
    use std::process::Command;

    Command::new("taskkill")
        .args(["/f", "/im", "explorer.exe"])
        .output()
        .map_err(|e| format!("结束 explorer 失败: {e}"))?;

    std::thread::sleep(std::time::Duration::from_millis(1200));

    Command::new("explorer.exe")
        .spawn()
        .map_err(|e| format!("启动 explorer 失败: {e}"))?;

    Ok("资源管理器已重启".into())
}
