//! Windows 右键菜单命令行工具，兼作 MCP server。
//!
//! 用法：
//!   ctxmenu list [--scope file] [--json]
//!   ctxmenu classic <status|on|off>
//!   ctxmenu disable <id>      禁用一个菜单项（可逆）
//!   ctxmenu enable <id>       重新启用
//!   ctxmenu delete <id> [--yes]  彻底删除（自动备份）
//!   ctxmenu create --scope file --name X --display "文字" --command "cmd" [--hive HKCU]
//!   ctxmenu restart-explorer
//!   ctxmenu mcp               以 MCP server 模式运行（stdio）
//!
//! 所有命令都支持 `--json`，方便程序/AI 解析。

mod mcp;

use std::process::ExitCode;

use ctx_menu_core::commands;
use ctx_menu_core::registry::{self, Hive, MenuItem, Scope};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.is_empty() || args[0] == "-h" || args[0] == "--help" || args[0] == "help" {
        print_help();
        return ExitCode::SUCCESS;
    }

    let command = args[0].as_str();
    let rest = &args[1..];

    let result = match command {
        "list" => cmd_list(rest),
        "classic" => cmd_classic(rest),
        "disable" => cmd_toggle(rest, true),
        "enable" => cmd_toggle(rest, false),
        "delete" => cmd_delete(rest),
        "create" => cmd_create(rest),
        "restart-explorer" => commands::restart_explorer().map(|m| {
            println!("{m}");
        }),
        "mcp" => {
            mcp::run();
            Ok(())
        }
        other => Err(format!("未知命令：{other}\n运行 ctxmenu --help 查看用法")),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("错误：{e}");
            ExitCode::FAILURE
        }
    }
}

fn print_help() {
    println!(
        "\
ctxmenu — Windows 右键菜单命令行工具 / MCP server

用法：
  ctxmenu list [--scope <scope>] [--blocked-only] [--json]
  ctxmenu classic [status|on|off]
  ctxmenu disable <id>
  ctxmenu enable <id>
  ctxmenu delete <id> [--yes]
  ctxmenu create --scope <scope> --name <键名> --display <文字> --command <命令> [--hive HKCU|HKLM]
  ctxmenu restart-explorer
  ctxmenu mcp

位置（scope）：
  file                  所有文件
  directory             文件夹
  directory_background  文件夹空白处
  desktop_background    桌面
  drive                 驱动器

id 格式：<HIVE>|<scope>|ext|verb|<名称>
  例：HKCU|file|ext|7-Zip

选项：
  --json    输出 JSON（便于程序解析）
  --yes     delete 时跳过确认

MCP 模式：
  在没有参数的标准输入/输出上运行 JSON-RPC，供 AI 客户端调用。
  配置示例（Claude Desktop / 其他 MCP 客户端）：
    {{\"command\": \"ctxmenu\", \"args\": [\"mcp\"]}}
"
    );
}

/// 解析 `--key value` 与 `--flag` 形式的参数。
fn parse_args(args: &[String]) -> (Vec<String>, std::collections::HashMap<String, String>) {
    let mut positional = Vec::new();
    let mut flags = std::collections::HashMap::new();
    let mut i = 0;

    while i < args.len() {
        let a = &args[i];
        if let Some(key) = a.strip_prefix("--") {
            // 下一个参数若不是 -- 开头，就当作它的值。
            if i + 1 < args.len() && !args[i + 1].starts_with("--") {
                flags.insert(key.to_string(), args[i + 1].clone());
                i += 2;
            } else {
                flags.insert(key.to_string(), String::new());
                i += 1;
            }
        } else {
            positional.push(a.clone());
            i += 1;
        }
    }
    (positional, flags)
}

fn resolve_scope(key: &str) -> Result<Scope, String> {
    Scope::from_key(key).ok_or_else(|| {
        format!("未知位置：{key}\n可用：file, directory, directory_background, desktop_background, drive")
    })
}

/// 按 id 或名称查找菜单项。id 精确匹配优先，其次按名称在所有位置里找。
fn find_item(query: &str) -> Result<MenuItem, String> {
    let items = registry::scan_all();

    if let Some(found) = items.iter().find(|i| i.id == query) {
        return Ok(found.clone());
    }

    // 退一步：按名称或显示名匹配，必须唯一。
    let matches: Vec<&MenuItem> = items
        .iter()
        .filter(|i| i.name == query || i.display_name == query)
        .collect();

    match matches.len() {
        0 => Err(format!(
            "找不到「{query}」。\n用 `ctxmenu list` 查看可用的 id。"
        )),
        1 => Ok(matches[0].clone()),
        n => {
            let list = matches
                .iter()
                .map(|i| format!("  {}", i.id))
                .collect::<Vec<_>>()
                .join("\n");
            Err(format!("「{query}」匹配到 {n} 个，请用完整 id 指定：\n{list}"))
        }
    }
}

fn cmd_list(args: &[String]) -> Result<(), String> {
    let (_, flags) = parse_args(args);

    let scope_filter = match flags.get("scope") {
        Some(s) if !s.is_empty() => Some(resolve_scope(s)?),
        _ => None,
    };
    let blocked_only = flags.contains_key("blocked-only");
    let as_json = flags.contains_key("json");

    let mut items = registry::scan_all();
    if let Some(scope) = scope_filter {
        items.retain(|i| i.scope == scope);
    }
    if blocked_only {
        items.retain(|i| i.blocked);
    }

    if as_json {
        let out = serde_json::json!({ "count": items.len(), "items": items });
        println!(
            "{}",
            serde_json::to_string_pretty(&out).map_err(|e| e.to_string())?
        );
        return Ok(());
    }

    if items.is_empty() {
        println!("没有匹配的菜单项。");
        return Ok(());
    }

    println!("共 {} 项\n", items.len());
    for it in &items {
        let state = if it.blocked { "已禁用" } else { "启用" };
        let kind = match it.kind {
            ctx_menu_core::ItemKind::ShellExtension => "扩展",
            ctx_menu_core::ItemKind::Verb => "命令",
        };
        let admin = if it.needs_admin { " *" } else { "" };
        println!(
            "  [{}]{}  {}  ({})  {}  {}",
            state,
            admin,
            it.display_name,
            kind,
            it.scope.label(),
            it.id
        );
        if let Some(t) = &it.target {
            println!("        → {t}");
        }
    }
    println!("\n* = 修改需要管理员权限");
    Ok(())
}

fn cmd_classic(args: &[String]) -> Result<(), String> {
    let (pos, flags) = parse_args(args);
    let as_json = flags.contains_key("json");
    let action = pos.first().map(|s| s.as_str()).unwrap_or("status");

    match action {
        "status" => {
            let st = commands::classic_state();
            if as_json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&st).map_err(|e| e.to_string())?
                );
            } else {
                println!("经典右键菜单：{}", if st.enabled { "已开启" } else { "未开启" });
                println!("  旧处理器 {{86ca1aa0}} 已屏蔽：{}", st.legacy_masked);
                println!("  新处理器 {{51668040}} 已屏蔽：{}", st.modern_masked);
                println!("  系统版本 build：{}", st.build);
                if st.needs_both_keys && !st.enabled {
                    println!("  提示：build >= 26100，两个键都必须写才能生效");
                }
            }
            Ok(())
        }
        "on" | "off" => {
            let enable = action == "on";
            let msg = commands::set_classic_menu(enable)?;
            if as_json {
                println!(
                    "{}",
                    serde_json::json!({ "ok": true, "enabled": enable, "message": msg })
                );
            } else {
                println!("{msg}");
                println!("提示：重启资源管理器后生效（ctxmenu restart-explorer）");
            }
            Ok(())
        }
        other => Err(format!("未知动作：{other}（可用：status, on, off）")),
    }
}

fn cmd_toggle(args: &[String], blocked: bool) -> Result<(), String> {
    let (pos, flags) = parse_args(args);
    let query = pos
        .first()
        .ok_or_else(|| "缺少菜单项 id。用 `ctxmenu list` 查看。".to_string())?;

    let item = find_item(query)?;
    let msg = commands::set_blocked(&item, blocked)?;

    if flags.contains_key("json") {
        println!(
            "{}",
            serde_json::json!({ "ok": true, "id": item.id, "blocked": blocked, "message": msg })
        );
    } else {
        println!("{msg}");
    }
    Ok(())
}

fn cmd_delete(args: &[String]) -> Result<(), String> {
    let (pos, flags) = parse_args(args);
    let query = pos
        .first()
        .ok_or_else(|| "缺少菜单项 id。用 `ctxmenu list` 查看。".to_string())?;

    let item = find_item(query)?;

    if item.is_system {
        return Err(format!(
            "「{}」是系统自带项，不提供删除。可以改用 `ctxmenu disable`。",
            item.display_name
        ));
    }

    let msg = commands::delete_item(&item)?;

    if flags.contains_key("json") {
        println!(
            "{}",
            serde_json::json!({ "ok": true, "id": item.id, "message": msg })
        );
    } else {
        println!("{msg}");
    }
    Ok(())
}

fn cmd_create(args: &[String]) -> Result<(), String> {
    let (_, flags) = parse_args(args);

    let need = |k: &str| -> Result<String, String> {
        flags
            .get(k)
            .filter(|v| !v.trim().is_empty())
            .cloned()
            .ok_or_else(|| format!("缺少 --{k}"))
    };

    let scope = resolve_scope(&need("scope")?)?;
    let name = need("name")?;
    let display = need("display")?;
    let command = need("command")?;

    let hive_label = flags.get("hive").cloned().unwrap_or_else(|| "HKCU".to_string());
    let hive = Hive::from_label(&hive_label)
        .ok_or_else(|| format!("未知配置单元：{hive_label}（可用 HKCU 或 HKLM）"))?;

    let msg = commands::create_verb(hive, scope, &name, &display, &command)?;

    if flags.contains_key("json") {
        println!(
            "{}",
            serde_json::json!({ "ok": true, "scope": scope.key(), "name": name, "message": msg })
        );
    } else {
        println!("{msg}");
    }
    Ok(())
}
