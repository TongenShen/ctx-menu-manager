//! MCP server（stdio 传输）。
//!
//! 协议：JSON-RPC 2.0，一行一条消息。实现 MCP 的核心子集：
//!   - initialize / notifications/initialized
//!   - tools/list
//!   - tools/call
//!
//! 手写而非引入 SDK，是因为这里只需要十来个工具、单线程顺序处理，
//! 引入 tokio 一整套异步运行时的收益不足以抵消体积和复杂度。

use std::io::{self, BufRead, Write};

use ctx_menu_core::commands;
use ctx_menu_core::registry::{self, Hive, MenuItem, Scope};
use serde_json::{json, Value};

const PROTOCOL_VERSION: &str = "2024-11-05";

pub fn run() {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }

        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let err = error_response(Value::Null, -32700, &format!("JSON 解析失败: {e}"));
                write_message(&mut stdout, &err);
                continue;
            }
        };

        // 通知（没有 id）不需要回复。
        let id = request.get("id").cloned();
        let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");

        if id.is_none() {
            // notifications/initialized 等，静默接受。
            continue;
        }

        let id = id.unwrap();
        let response = match method {
            "initialize" => ok_response(
                id,
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": { "tools": {} },
                    "serverInfo": {
                        "name": "ctxmenu",
                        "version": env!("CARGO_PKG_VERSION"),
                    }
                }),
            ),
            "tools/list" => ok_response(id, json!({ "tools": tool_definitions() })),
            "tools/call" => match handle_tool_call(&request) {
                Ok(result) => ok_response(id, result),
                Err(e) => error_response(id, -32603, &e),
            },
            "ping" => ok_response(id, json!({})),
            other => error_response(id, -32601, &format!("不支持的方法: {other}")),
        };

        write_message(&mut stdout, &response);
    }
}

fn write_message(out: &mut io::Stdout, msg: &Value) {
    let _ = writeln!(out, "{}", msg);
    let _ = out.flush();
}

fn ok_response(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}

/// 工具定义。description 写得具体些，AI 才知道什么时候该用哪个工具。
fn tool_definitions() -> Value {
    json!([
        {
            "name": "list_menu_items",
            "description": "列出 Windows 右键菜单项。返回每项的 id、显示名、类型（shell_extension/verb）、位置、是否已禁用、是否需要管理员权限。先用这个拿到 id，再用其他工具操作。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "scope": {
                        "type": "string",
                        "enum": ["file", "directory", "directory_background", "desktop_background", "drive"],
                        "description": "只看某个位置的项。省略则返回全部。"
                    },
                    "blocked_only": { "type": "boolean", "description": "只返回已禁用的项。" }
                }
            }
        },
        {
            "name": "classic_menu_status",
            "description": "查询经典（Windows 10 风格）右键菜单是否已开启，以及两个 CLSID 键的状态和系统 build 号。",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "set_classic_menu",
            "description": "开启或关闭经典右键菜单。Windows 11 build 26100 及以上必须同时写 {86ca1aa0} 和 {51668040} 两个 CLSID，本工具会自动处理。改完需重启资源管理器才生效。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "enable": { "type": "boolean", "description": "true 开启经典菜单，false 还原成 Win11 新菜单。" }
                },
                "required": ["enable"]
            }
        },
        {
            "name": "disable_menu_item",
            "description": "禁用一个右键菜单项，可逆。shell 扩展走 HKCU 的 Shell Extensions\\Blocked 列表（不需提权），verb 项写 LegacyDisable。改完需重启资源管理器。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "菜单项 id，形如 HKCU|file|ext|7-Zip" }
                },
                "required": ["id"]
            }
        },
        {
            "name": "enable_menu_item",
            "description": "重新启用一个被禁用的菜单项，撤销 disable_menu_item 的效果。改完需重启资源管理器。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "菜单项 id" }
                },
                "required": ["id"]
            }
        },
        {
            "name": "delete_menu_item",
            "description": "彻底删除一个右键菜单项。删除前自动把整个注册表键导出为 .reg 备份到 %APPDATA%\\CtxMenuManager\\backup\\。系统自带项会被拒绝。这是不可逆操作，临时关闭请用 disable_menu_item。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "菜单项 id" }
                },
                "required": ["id"]
            }
        },
        {
            "name": "create_menu_item",
            "description": "新建一个命令行右键菜单项，写入 shell\\<name>\\command。命令里用 %1 表示选中的文件，%V 表示当前文件夹。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "scope": {
                        "type": "string",
                        "enum": ["file", "directory", "directory_background", "desktop_background", "drive"],
                        "description": "菜单项出现的位置。"
                    },
                    "name": { "type": "string", "description": "注册表键名，英文无空格，例如 OpenWithVSCode" },
                    "display": { "type": "string", "description": "菜单上显示的文字，例如「用 VS Code 打开」" },
                    "command": { "type": "string", "description": "要执行的命令行" },
                    "hive": {
                        "type": "string",
                        "enum": ["HKCU", "HKLM"],
                        "description": "写入哪个配置单元。默认 HKCU（当前用户，不需提权）；HKLM 需要管理员权限。"
                    }
                },
                "required": ["scope", "name", "display", "command"]
            }
        },
        {
            "name": "restart_explorer",
            "description": "重启 Windows 资源管理器，让注册表改动生效。会关闭所有已打开的文件夹窗口，正在运行的程序不受影响。",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "system_info",
            "description": "返回 Windows 版本、build 号，以及是否属于需要双 CLSID 的版本（build >= 26100）。",
            "inputSchema": { "type": "object", "properties": {} }
        }
    ])
}

fn handle_tool_call(request: &Value) -> Result<Value, String> {
    let params = request.get("params").ok_or("缺少 params")?;
    let name = params.get("name").and_then(|n| n.as_str()).ok_or("缺少工具名")?;
    let args = params.get("arguments").cloned().unwrap_or(json!({}));

    let result = match name {
        "list_menu_items" => tool_list(&args),
        "classic_menu_status" => tool_classic_status(),
        "set_classic_menu" => tool_set_classic(&args),
        "disable_menu_item" => tool_toggle(&args, true),
        "enable_menu_item" => tool_toggle(&args, false),
        "delete_menu_item" => tool_delete(&args),
        "create_menu_item" => tool_create(&args),
        "restart_explorer" => commands::restart_explorer().map(|m| json!({ "ok": true, "message": m })),
        "system_info" => {
            let info = ctx_menu_core::system_info();
            Ok(json!({
                "product": info.product,
                "display": info.display,
                "build": info.build,
                "ubr": info.ubr,
                "needs_both_clsid_keys": info.needs_both_keys(),
            }))
        }
        other => Err(format!("未知工具: {other}")),
    };

    match result {
        Ok(value) => Ok(json!({
            "content": [{ "type": "text", "text": serde_json::to_string_pretty(&value).unwrap_or_default() }],
            "isError": false
        })),
        Err(e) => Ok(json!({
            "content": [{ "type": "text", "text": e }],
            "isError": true
        })),
    }
}

fn tool_list(args: &Value) -> Result<Value, String> {
    let mut items = registry::scan_all();

    if let Some(scope) = args.get("scope").and_then(|s| s.as_str()) {
        let scope = Scope::from_key(scope).ok_or_else(|| format!("未知位置: {scope}"))?;
        items.retain(|i| i.scope == scope);
    }
    if args.get("blocked_only").and_then(|b| b.as_bool()).unwrap_or(false) {
        items.retain(|i| i.blocked);
    }

    Ok(json!({ "count": items.len(), "items": items }))
}

fn tool_classic_status() -> Result<Value, String> {
    let st = commands::classic_state();
    Ok(serde_json::to_value(&st).map_err(|e| e.to_string())?)
}

fn tool_set_classic(args: &Value) -> Result<Value, String> {
    let enable = args
        .get("enable")
        .and_then(|b| b.as_bool())
        .ok_or("缺少 enable 参数")?;
    let msg = commands::set_classic_menu(enable)?;
    Ok(json!({ "ok": true, "enabled": enable, "message": msg }))
}

fn find_item(query: &str) -> Result<MenuItem, String> {
    let items = registry::scan_all();
    if let Some(found) = items.iter().find(|i| i.id == query) {
        return Ok(found.clone());
    }
    let matches: Vec<&MenuItem> = items
        .iter()
        .filter(|i| i.name == query || i.display_name == query)
        .collect();
    match matches.len() {
        0 => Err(format!("找不到「{query}」")),
        1 => Ok(matches[0].clone()),
        n => Err(format!(
            "「{query}」匹配到 {n} 个，请用完整 id：{}",
            matches.iter().map(|i| i.id.clone()).collect::<Vec<_>>().join(", ")
        )),
    }
}

fn tool_toggle(args: &Value, blocked: bool) -> Result<Value, String> {
    let id = args.get("id").and_then(|s| s.as_str()).ok_or("缺少 id 参数")?;
    let item = find_item(id)?;
    let msg = commands::set_blocked(&item, blocked)?;
    Ok(json!({ "ok": true, "id": item.id, "blocked": blocked, "message": msg }))
}

fn tool_delete(args: &Value) -> Result<Value, String> {
    let id = args.get("id").and_then(|s| s.as_str()).ok_or("缺少 id 参数")?;
    let item = find_item(id)?;
    if item.is_system {
        return Err(format!(
            "「{}」是系统自带项，不提供删除。可以用 disable_menu_item 临时关闭。",
            item.display_name
        ));
    }
    let msg = commands::delete_item(&item)?;
    Ok(json!({ "ok": true, "id": item.id, "message": msg }))
}

fn tool_create(args: &Value) -> Result<Value, String> {
    let get = |k: &str| -> Result<String, String> {
        args.get(k)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| format!("缺少 {k} 参数"))
    };

    let scope = Scope::from_key(&get("scope")?).ok_or("未知位置")?;
    let name = get("name")?;
    let display = get("display")?;
    let command = get("command")?;

    let hive_label = args
        .get("hive")
        .and_then(|v| v.as_str())
        .unwrap_or("HKCU");
    let hive = Hive::from_label(hive_label).ok_or("未知配置单元")?;

    let msg = commands::create_verb(hive, scope, &name, &display, &command)?;
    Ok(json!({ "ok": true, "scope": scope.key(), "name": name, "message": msg }))
}
