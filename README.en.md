# Right-Click Menu Manager

**A graphical tool for managing the Windows 11 right-click menu — restore the classic Windows 10 layout, and enable, hide or delete individual entries.**

No registry paths to memorize, no commands to type. Toggle, click, restart Explorer, done.

[English](README.en.md) · [中文](README.md)

![Version](https://img.shields.io/badge/version-2.0.0-blue)
![Platform](https://img.shields.io/badge/platform-Windows%2011-0078D6)
![License](https://img.shields.io/badge/license-MIT-green)

---

## The problem

Since 21H2, Windows 11 has shipped a compact context menu that buries shell extensions — 7-Zip, Git, cloud drives, IDEs — behind a "Show more options" entry. Want to remove a useless entry, or get the classic menu back? Microsoft offers no interface for it. You edit the registry by hand.

It got worse in October 2026: **the five-year-old registry workaround stopped working.** KB5127936 rewrote the context menu engine, and after KB5132061 (Build 26300.9974 / 26200.9974 / 26100.9974) a single empty value under `{86ca1aa0}` is no longer enough — you must also mask `{51668040}`. Most guides online predate this and simply fail.

This tool handles all of it, and **shows you exactly which registry key each action touches**.

---

## Features

### 1. Classic menu toggle

One click in the header switches between the Windows 10 classic menu and the Windows 11 modern menu. It writes both CLSIDs required on 26H2, and it detects the half-applied state where only one key exists — reporting "config incomplete (missing key)" instead of leaving you guessing.

Hit "Restart Explorer" and it takes effect immediately.

### 2. Per-entry management

Entries are grouped into five locations. Each row shows the friendly name, registry key name, entry type, hive, and the actual DLL/exe behind it:

| Location | Meaning |
|---|---|
| Files | Right-clicking any file |
| Folders | Right-clicking a folder |
| Folder background | Right-clicking empty space inside a folder |
| Desktop | Right-clicking the desktop background |
| Drives | Right-clicking a disk or USB drive |

Hover a location tab to see its registry path (can be disabled in Settings).

### 3. Two ways to disable

You choose what the "disable" action does:

| Mode | Mechanism | When to use |
|---|---|---|
| **Mark as hidden** (default) | Extensions: add the CLSID to the HKCU blocklist<br>Verbs: add a `LegacyDisable` value | Temporarily off, fully reversible, nothing lost |
| **Back up and delete** | Exports `.reg`, then removes the key | You are sure, and want it gone |

The two entry types are disabled by **different mechanisms**, which is an easy trap: Windows only honours CLSIDs in the `Shell Extensions\Blocked` list for shell extensions, and only the `LegacyDisable` value for verbs. Writing a `Blocked` value under the key does nothing.

Before any deletion the entire key (including subkeys) is exported to `%APPDATA%\CtxMenuManager\backup\` with a timestamp in the filename. Import it back if something breaks.

### 4. Create new entries

Pick a location, type a key name, display text and command — it writes `shell\<name>\command` for you. Handy for adding "Open with VS Code" to every file.

### 5. Interface

- Six colour themes: Midnight, Graphite, Violet, Teal, Ember, Daylight
- Search, show-enabled-only, hide system entries
- Preferences are stored locally and restored next launch

---

## Download

Get the latest from [Releases](https://github.com/TongenShen/ctx-menu-manager/releases):

| File | Description |
|---|---|
| `ctx-menu-manager_2.0.0_x64-setup.exe` | **Installer.** Installs per-user, adds a Start menu shortcut, uninstalls from Apps & Features. |
| `ctx-menu-manager_2.0.0_x64-portable.zip` | **Portable.** Unzip and run the exe. No install records, USB-friendly. |

Both are feature-identical — the only difference is whether you go through an installer.

---

## Usage

1. Launch the app; the header shows the current classic-menu state
2. Want the Windows 10 menu back? Click "Enable", then "Restart Explorer"
3. Want to clean up an entry? Pick its location tab, find it, click "Disable" or "Delete"
4. Click "Restart Explorer" to reload File Explorer

**Note:** restarting Explorer closes all open folder windows. Running programs are unaffected.

---

## Requirements

| Item | Requirement |
|---|---|
| OS | Windows 11 (Build 22000 or later) |
| Architecture | x64 |
| Runtime | WebView2 Runtime (bundled with Windows 11) |
| Privileges | **No administrator rights needed** — writes to the current user hive (HKCU) only |

> Windows 10 and earlier do not need this tool; their context menu is already the classic one.

---

## Safety

**HKCU only.** Every write goes to `HKEY_CURRENT_USER\Software\Classes`. Nothing touches `HKEY_LOCAL_MACHINE`, other user accounts are unaffected, and no elevation is requested.

**System entries are protected.** Entries pointing into `System32` / `SysWOW64` are marked "system" and their delete button is disabled, so Windows built-ins are not removed by accident.

**Deletion is backed up.** A full `.reg` export precedes every removal, and the confirmation dialog shows you where it goes.

**Still, be careful.** Registry edits carry inherent risk. If you are unsure what an entry does, use "Mark as hidden" first — it is always reversible. When in doubt, do not delete.

---

## Building from source

```powershell
# Requires Rust 1.77+, Node.js 18+, Visual Studio Build Tools
git clone https://github.com/TongenShen/ctx-menu-manager.git
cd ctx-menu-manager
npm install
npm run build
```

Artifacts land in `src-tauri/target/release/bundle/`.

For development:

```powershell
npm run dev
```

> Running `npx tauri dev` from MSYS2 or Git Bash fails with `Cannot find native binding`, because the Tauri CLI ships msvc bindings only. Use PowerShell or cmd.

---

## FAQ

**I enabled the classic menu but nothing changed.**
Restart Explorer first. If it still does not work, check whether only one CLSID got written — the settings panel reports "config incomplete" in that case. The app writes the second key automatically.

**I applied the registry tweak before and it stopped working.**
The October 2026 update broke the single-key approach. Run this tool's toggle once; it writes both keys.

**How do I undo a deletion?**
Find the matching timestamped `.reg` in `%APPDATA%\CtxMenuManager\backup\` and double-click to import. Or just use "Mark as hidden", which never needs restoring.

**Does it need administrator rights?**
No. Everything is scoped to the current user. Elevation is only needed if you deliberately choose to write to HKLM, which the UI labels.

---

## Author

**TongenShen**

- GitHub: https://github.com/TongenShen
- Project: https://github.com/TongenShen/ctx-menu-manager

## License

[MIT](LICENSE) — free to use, modify and redistribute.

Provided as-is, without warranty. Registry edits carry risk; consider creating a restore point first.
