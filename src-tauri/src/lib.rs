// Windows 右键菜单管理器 —— Tauri 后端库入口。

pub mod commands;
pub mod registry;

use commands::ClassicMenuState;
use registry::{Hive, MenuItem, Scope};

/// 列出所有右键菜单项。
#[tauri::command]
fn list_items() -> Vec<MenuItem> {
    registry::scan_all()
}

/// 某个文件是否还存在（用于标记失效项）。
#[tauri::command]
fn check_target_exists(target: String) -> bool {
    registry::target_exists(&target)
}

/// 临时开关：设置或清除 Blocked 标记。
#[tauri::command]
fn set_item_blocked(item: MenuItem, blocked: bool) -> Result<String, String> {
    commands::set_blocked(&item, blocked)
}

/// 彻底删除某个菜单项（自动备份）。
#[tauri::command]
fn delete_item(item: MenuItem) -> Result<String, String> {
    commands::delete_item(&item)
}

/// 新建 verb 菜单项。
#[tauri::command]
fn create_verb(
    hive: String,
    scope: String,
    name: String,
    display: String,
    command: String,
) -> Result<String, String> {
    let hive = Hive::from_label(&hive).ok_or_else(|| format!("未知配置单元: {hive}"))?;
    let scope = Scope::from_key(&scope).ok_or_else(|| format!("未知位置: {scope}"))?;
    commands::create_verb(hive, scope, &name, &display, &command)
}

/// 查询经典菜单状态。
#[tauri::command]
fn get_classic_state() -> ClassicMenuState {
    commands::classic_state()
}

/// 开启/关闭经典菜单。
#[tauri::command]
fn set_classic_menu(enable: bool) -> Result<String, String> {
    commands::set_classic_menu(enable)
}

/// 重启资源管理器。
#[tauri::command]
fn restart_explorer() -> Result<String, String> {
    commands::restart_explorer()
}

/// 用默认浏览器打开链接（仅 http/https）。
#[tauri::command]
fn open_url(url: String) -> Result<String, String> {
    commands::open_url(&url)
}

/// 当前的系统版本信息，用于界面提示。
#[tauri::command]
fn get_system_info() -> serde_json::Value {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    let key = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion")
        .ok();

    let get = |name: &str| -> String {
        key.as_ref()
            .and_then(|k| k.get_value::<String, _>(name).ok())
            .unwrap_or_default()
    };

    serde_json::json!({
        "product": get("ProductName"),
        "display": get("DisplayVersion"),
        "build": get("CurrentBuildNumber"),
        "ubr": get("UBR"),
    })
}

/// 应用入口，供 main.rs 与移动端调用。
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            list_items,
            check_target_exists,
            set_item_blocked,
            delete_item,
            create_verb,
            get_classic_state,
            set_classic_menu,
            restart_explorer,
            open_url,
            get_system_info,
        ])
        .run(tauri::generate_context!())
        .expect("启动 Tauri 应用失败");
}
