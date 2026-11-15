# CLI 与 MCP 使用说明

`ctxmenu` 是右键菜单管理器的命令行版本，同时兼作 **MCP server**，让 AI 助手（Claude Desktop、Cursor、DeepSeek Harness 等）能直接管理你的右键菜单。

GUI 和 CLI 共用同一套注册表逻辑（`core/`），行为完全一致。

---

## 一、先搞清楚：为什么不让 AI 直接翻注册表？

完全可以，但**做修改时不建议**。原因是右键菜单分散在至少十个不同位置，而且「禁用」的机制按类型分岔：

| 菜单项类型 | 正确的禁用方式 |
|---|---|
| Shell 扩展（COM） | 把 **CLSID** 加入 `HKCU\Software\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked` |
| Verb 命令项 | 在 verb 键下加 `LegacyDisable` |

**在键下写 `Blocked` 值是无效的**——这是本项目的 v1.0.0 实际踩过的坑，改了但完全没生效。

用 CLI/MCP 的好处：

- AI 不用拼注册表路径，`ctxmenu list` 直接给出每项的 id 和真实状态
- 机制不会用错，两类各走各的正确写法
- 输出是结构化 JSON，不会解析歧义
- 删除自动备份，系统项自动拒绝

**只读查看**（比如"我的右键菜单里都有什么"）直接翻注册表也行，CLI 只是更省事。

---

## 二、安装

CLI 是独立的小程序，不需要装 GUI。从 [Releases](https://github.com/TongenShen/ctx-menu-manager/releases) 下载 `ctxmenu_2.1.0_x64.zip`，解压出 `ctxmenu.exe`，放到 `PATH` 里的任意目录（比如 `C:\Users\你\bin\`）。

验证：

```powershell
ctxmenu --help
```

---

## 三、CLI 用法

### 查看

```powershell
# 列出全部菜单项
ctxmenu list

# 只看某个位置
ctxmenu list --scope file
ctxmenu list --scope directory
ctxmenu list --scope directory_background
ctxmenu list --scope desktop_background
ctxmenu list --scope drive

# 只看已禁用的
ctxmenu list --blocked-only

# JSON 输出（给脚本/AI 用）
ctxmenu list --json
```

输出示例：

```
共 17 项

  [启用]  通过 Code 打开  (命令)  文件  HKCU|file|verb|VSCode
        → C:\Users\...\Code.exe
  [启用] *  7-Zip Shell Extension  (扩展)  文件  HKLM|file|ext|7-Zip
        → C:\Program Files\7-Zip\7-zip.dll

* = 修改需要管理员权限
```

### 禁用 / 启用（可逆）

```powershell
ctxmenu disable "HKLM|file|ext|7-Zip"
ctxmenu enable  "HKLM|file|ext|7-Zip"
```

改完需要重启资源管理器才看得到效果：

```powershell
ctxmenu restart-explorer
```

### 删除（不可逆，自动备份）

```powershell
ctxmenu delete "HKCU|file|verb|VSCode"
```

删除前会把整个注册表键导出到 `%APPDATA%\CtxMenuManager\backup\`，文件名带时间戳。系统自带项会被拒绝。

**临时关闭请用 `disable`，不要用 `delete`。**

### 新建

```powershell
ctxmenu create --scope file --name OpenWithNotepad `
  --display "用记事本打开" --command 'notepad.exe "%1"'
```

`--hive HKLM` 需要管理员权限，默认写 HKCU（只影响当前用户）。

### 经典菜单

```powershell
ctxmenu classic status   # 查询状态
ctxmenu classic on       # 恢复 Win10 经典菜单
ctxmenu classic off      # 还原 Win11 新菜单
ctxmenu restart-explorer
```

build 26100（24H2/25H2/26H2）及以上会自动写两个 CLSID，不需要手动处理。

---

## 四、MCP 接入

### 启动方式

MCP 走 stdio，客户端负责拉起进程：

```json
{
  "mcpServers": {
    "ctxmenu": {
      "command": "ctxmenu",
      "args": ["mcp"]
    }
  }
}
```

如果 `ctxmenu` 不在 `PATH` 里，写绝对路径：

```json
{
  "mcpServers": {
    "ctxmenu": {
      "command": "C:\\Users\\你\\bin\\ctxmenu.exe",
      "args": ["mcp"]
    }
  }
}
```

**Claude Desktop**：编辑 `%APPDATA%\Claude\claude_desktop_config.json`，把上面那段并进 `mcpServers`，重启 Claude。

**DeepSeek Harness**：在 MCP 服务器配置里添加同样一条（command + args 形式）。

**Cursor**：`.cursor/mcp.json`，格式相同。

### 提供的工具

| 工具 | 作用 |
|---|---|
| `list_menu_items` | 列出菜单项（可按位置筛选、只看已禁用） |
| `classic_menu_status` | 查询经典菜单是否开启，含 build 号 |
| `set_classic_menu` | 开启/关闭经典菜单 |
| `disable_menu_item` | 禁用一个项（可逆） |
| `enable_menu_item` | 重新启用 |
| `delete_menu_item` | 彻底删除（自动备份，系统项拒绝） |
| `create_menu_item` | 新建命令行菜单项 |
| `restart_explorer` | 重启资源管理器 |
| `system_info` | 系统版本与是否需要双 CLSID |

工具描述里写清了"改完需重启资源管理器"这类前提，AI 一般会自己接上这一步。

### 典型对话

> **你**：把右键菜单里那些我不认识的项列出来
>
> *AI 调 `list_menu_items`，读出列表，按类型和来源解释*

> **你**：7-Zip 那个暂时别显示了，但我可能还要用回来
>
> *AI 调 `disable_menu_item`（而不是 delete），然后 `restart_explorer`*

> **你**：右键变回 Win11 那样吧
>
> *AI 调 `set_classic_menu(false)` + `restart_explorer`*

---

## 五、安全边界

- **默认只写 HKCU**：不影响其他用户，不需要管理员权限
- **删除有备份**：每次删除前导出完整 `.reg`
- **系统项受保护**：指向 `System32` / `SysWOW64` 的项不允许删除
- **需要提权的操作会失败并说明原因**：不会静默失败

AI 通过 MCP 调用时同样受这些限制约束——它拿不到额外的权限。

---

## 六、开发者：从源码构建

```powershell
git clone https://github.com/TongenShen/ctx-menu-manager.git
cd ctx-menu-manager

cargo build -p ctxmenu --release        # 只构建 CLI
cargo build -p ctx-menu-manager --release  # 构建 GUI
```

产物：
- CLI：`target/release/ctxmenu.exe`
- GUI：`target/release/ctx-menu-manager.exe`

项目结构：

```
ctx-menu-manager/
├── core/        纯注册表逻辑，不依赖任何 UI 框架
├── cli/         CLI + MCP server（依赖 core）
└── src-tauri/   Tauri GUI（依赖 core）
```

三层共用 `core`，所以 CLI 和 GUI 的行为永远一致。
