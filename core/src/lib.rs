//! Windows 右键菜单的读写逻辑。
//!
//! 这一层不依赖 tauri，也不依赖任何 UI 框架：GUI、CLI、MCP server 三者共用它。
//! 所有函数都是同步的、直接操作注册表。

pub mod commands;
pub mod registry;

pub use commands::ClassicMenuState;
pub use registry::{Hive, ItemKind, MenuItem, Scope};

/// 系统版本信息，用于判断是否需要 26H2 的双 CLSID 处理。
#[derive(Debug, Clone, serde::Serialize)]
pub struct SystemInfo {
    pub product: String,
    pub display: String,
    pub build: String,
    pub ubr: String,
}

impl SystemInfo {
    pub fn build_number(&self) -> u32 {
        self.build.parse().unwrap_or(0)
    }

    /// build >= 26100（24H2 及以后）需要同时写两个 CLSID。
    pub fn needs_both_keys(&self) -> bool {
        self.build_number() >= 26100
    }
}

pub fn system_info() -> SystemInfo {
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

    SystemInfo {
        product: get("ProductName"),
        display: get("DisplayVersion"),
        build: get("CurrentBuildNumber"),
        ubr: get("UBR"),
    }
}
