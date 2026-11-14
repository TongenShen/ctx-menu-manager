// 右键菜单管理器 —— 前端逻辑
const { invoke } = window.__TAURI__.core;

// ---------- 设置：持久化与主题 ----------
const THEMES = [
  { key: 'midnight', name: '午夜蓝', colors: ['#16171b', '#1d1f24', '#5e9eff', '#8b5cf6'] },
  { key: 'graphite', name: '石墨灰', colors: ['#1a1a1a', '#212121', '#b8bcc2', '#6b7280'] },
  { key: 'violet',   name: '暗紫',   colors: ['#16121e', '#1c1727', '#a78bfa', '#f472b6'] },
  { key: 'teal',     name: '墨绿',   colors: ['#0f1a18', '#14211f', '#2dd4bf', '#38bdf8'] },
  { key: 'ember',    name: '暖橙',   colors: ['#1c1614', '#241c19', '#e08a4b', '#d97706'] },
  { key: 'daylight', name: '日光白', colors: ['#f5f6f8', '#ffffff', '#2563eb', '#7c3aed'] },
];

const SETTINGS_KEY = 'ctxmm.settings';

const DEFAULTS = {
  theme: 'midnight',
  disableMode: 'blocked', // blocked = 标记隐藏；delete = 备份后删除
  tooltip: true,
};

const settings = { ...DEFAULTS };

function loadSettings() {
  try {
    const raw = localStorage.getItem(SETTINGS_KEY);
    if (raw) Object.assign(settings, JSON.parse(raw));
  } catch { /* 读不出来就用默认值 */ }
  // 主题失效时回退
  if (!THEMES.some((t) => t.key === settings.theme)) settings.theme = DEFAULTS.theme;
}

function saveSettings() {
  try {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
  } catch { /* 存不了就算了，不影响使用 */ }
}

function applyTheme(key) {
  document.documentElement.setAttribute('data-theme', key);
}

function applyTooltipSetting() {
  document.documentElement.dataset.tooltip = settings.tooltip ? 'on' : 'off';
}

loadSettings();
applyTheme(settings.theme);
applyTooltipSetting();

const SCOPES = [
  {
    key: 'file',
    label: '文件',
    hint: '对任意文件右键时出现的菜单项',
    reg: '*\\shell 与 *\\shellex\\ContextMenuHandlers',
  },
  {
    key: 'directory',
    label: '文件夹',
    hint: '对文件夹（目录）右键时出现的菜单项',
    reg: 'Directory\\shell 与 Directory\\shellex',
  },
  {
    key: 'directory_background',
    label: '文件夹空白处',
    hint: '打开文件夹后，在空白区域右键时出现的菜单项',
    reg: 'Directory\\Background\\shell 与 \\shellex',
  },
  {
    key: 'desktop_background',
    label: '桌面',
    hint: '在桌面空白处右键时出现的菜单项',
    reg: 'DesktopBackground\\shell 与 \\shellex',
  },
  {
    key: 'drive',
    label: '驱动器',
    hint: '对磁盘分区、U 盘等驱动器右键时出现的菜单项',
    reg: 'Drive\\shell 与 Drive\\shellex',
  },
];

const state = {
  items: [],
  scope: 'file',
  search: '',
  onlyEnabled: false,
  hideSystem: false,
  classic: null,
  busy: false,
};

const $ = (id) => document.getElementById(id);

// ---------- 悬停说明浮层 ----------
// 用自定义浮层而非原生 title：原生要等 1 秒多才出现，且不能控制样式。
(function setupTooltip() {
  const tip = () => $('tip');
  let current = null;

  function show(target) {
    // 设置里关掉悬停说明时，直接不显示。
    if (!settings.tooltip) return;
    const text = target.dataset.tip;
    if (!text) return;
    current = target;

    const el = tip();
    // 把注册表路径那几行用等宽字体高亮出来，纯文本行保持普通样式。
    const lines = text.split('\n');
    el.innerHTML = '';
    for (const line of lines) {
      if (/^HKCU\/HKLM\\Software\\Classes\\/.test(line)) {
        const span = document.createElement('span');
        span.className = 'tip-mono';
        span.textContent = line;
        el.appendChild(span);
      } else {
        el.appendChild(document.createTextNode(line));
      }
      el.appendChild(document.createTextNode('\n'));
    }

    el.hidden = false;
    position(target);
  }

  function position(target) {
    const el = tip();
    const r = target.getBoundingClientRect();
    const box = el.getBoundingClientRect();

    let left = r.left;
    let top = r.bottom + 8;

    // 右边界收拢
    if (left + box.width > window.innerWidth - 10) {
      left = window.innerWidth - box.width - 10;
    }
    if (left < 8) left = 8;

    // 下方空间不足则翻到上方
    if (top + box.height > window.innerHeight - 10) {
      top = r.top - box.height - 8;
    }

    el.style.left = `${Math.max(8, left)}px`;
    el.style.top = `${Math.max(8, top)}px`;
  }

  function hide() {
    current = null;
    tip().hidden = true;
  }

  document.addEventListener('mouseover', (e) => {
    const t = e.target.closest('[data-tip]');
    if (t && t !== current) show(t);
    else if (!t && current) hide();
  });

  document.addEventListener('mouseout', (e) => {
    const t = e.target.closest('[data-tip]');
    if (t && !t.contains(e.relatedTarget)) hide();
  });

  // 点击、滚动、窗口变化时收起，避免浮层残留。
  document.addEventListener('click', hide);
  window.addEventListener('scroll', hide, true);
  window.addEventListener('resize', hide);
})();

// ---------- 提示 ----------
let toastTimer = null;
function toast(msg, kind = 'ok') {
  const el = $('toast');
  el.textContent = msg;
  el.className = `toast ${kind}`;
  el.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { el.hidden = true; }, kind === 'err' ? 6000 : 3200);
}

// ---------- 系统信息与经典菜单状态 ----------
async function loadSystemInfo() {
  try {
    const info = await invoke('get_system_info');
    const ubr = info.ubr ? `.${info.ubr}` : '';
    $('sysinfo').textContent =
      `${info.product || 'Windows'} · ${info.display || ''} · Build ${info.build}${ubr}`;
  } catch (e) {
    $('sysinfo').textContent = '无法读取系统信息';
  }
}

async function loadClassicState() {
  try {
    const s = await invoke('get_classic_state');
    state.classic = s;
    const label = $('classicState');
    const btn = $('btnClassic');

    if (s.enabled) {
      label.textContent = '已开启（Win10 样式）';
      label.className = 'classic-state on';
      btn.textContent = '还原成新菜单';
      btn.classList.remove('btn-primary');
    } else if (s.legacy_masked || s.modern_masked) {
      // 只写了一个键 —— 26H2 上必然无效，这是常见困惑点。
      label.textContent = '配置不完整（缺键，无效）';
      label.className = 'classic-state off';
      btn.textContent = '修复';
      btn.classList.add('btn-primary');
    } else {
      label.textContent = '未开启（Win11 新菜单）';
      label.className = 'classic-state off';
      btn.textContent = '开启';
      btn.classList.add('btn-primary');
    }
  } catch (e) {
    $('classicState').textContent = '读取失败';
  }
}

// ---------- 列表渲染 ----------
function scopesWithCounts() {
  const counts = {};
  for (const it of state.items) {
    counts[it.scope] = (counts[it.scope] || 0) + 1;
  }
  return SCOPES.map((s) => ({ ...s, count: counts[s.key] || 0 }));
}

function renderScopes() {
  const nav = $('scopes');
  nav.innerHTML = '';
  for (const s of scopesWithCounts()) {
    const b = document.createElement('button');
    b.className = `scope-tab${s.key === state.scope ? ' active' : ''}`;
    b.innerHTML = `${s.label}<span class="count">${s.count}</span>`;

    // 悬停说明：用途 + 对应的注册表路径 + 当前数量。
    b.dataset.tip =
      `${s.hint}\n\nHKCU/HKLM\\Software\\Classes\\${s.reg}\n\n当前共 ${s.count} 项`;

    b.onclick = () => {
      state.scope = s.key;
      renderScopes();
      renderList();
    };
    nav.appendChild(b);
  }
}

function visibleItems() {
  const q = state.search.trim().toLowerCase();
  return state.items.filter((it) => {
    if (it.scope !== state.scope) return false;
    if (state.onlyEnabled && it.blocked) return false;
    if (state.hideSystem && it.is_system) return false;
    if (!q) return true;
    return (
      it.display_name.toLowerCase().includes(q) ||
      it.name.toLowerCase().includes(q) ||
      (it.target || '').toLowerCase().includes(q) ||
      (it.clsid || '').toLowerCase().includes(q)
    );
  });
}

function el(tag, className, text) {
  const e = document.createElement(tag);
  if (className) e.className = className;
  if (text !== undefined) e.textContent = text;
  return e;
}

function renderList() {
  const list = $('list');
  const items = visibleItems();
  list.innerHTML = '';
  $('empty').hidden = items.length > 0;

  for (const it of items) {
    const row = el('div', `row${it.blocked ? ' blocked' : ''}`);

    // 名称
    const nameCell = el('div', 'name-cell');
    const nameLine = el('div', 'name-text');
    nameLine.textContent = it.display_name;
    if (it.is_system) {
      const sys = el('span', 'badge badge-sys', '系统');
      nameLine.appendChild(sys);
    }
    nameCell.appendChild(nameLine);
    nameCell.appendChild(el('div', 'name-key', it.name));
    row.appendChild(nameCell);

    // 类型
    const kindCell = el('div');
    kindCell.appendChild(
      el('span', `badge ${it.kind === 'shell_extension' ? 'badge-ext' : 'badge-verb'}`,
        it.kind === 'shell_extension' ? '扩展' : '命令')
    );
    row.appendChild(kindCell);

    // Hive
    const hiveCell = el('div');
    const hiveBadge = el('span', `badge badge-hive${it.needs_admin ? ' admin' : ''}`, it.hive);
    if (it.needs_admin) hiveBadge.title = '需要管理员权限才能修改';
    hiveCell.appendChild(hiveBadge);
    row.appendChild(hiveCell);

    // 目标
    const targetCell = el('div', 'target', it.target || '—');
    targetCell.title = it.target || '';
    row.appendChild(targetCell);

    // 操作
    const actions = el('div', 'actions');

    if (settings.disableMode === 'delete') {
      // 删除模式：只有「移除」一个按钮，点了就是备份后删除，条目会消失。
      const rm = el('button', 'btn-sm danger', '移除');
      rm.title = it.is_system
        ? '系统自带项，不提供移除'
        : '导出 .reg 备份后删除该注册表键（不可恢复，除非导入备份）';
      rm.disabled = it.is_system;
      rm.onclick = () => removeItem(it);
      actions.appendChild(rm);
    } else {
      // 标记模式：临时开关 + 单独的彻底删除。
      const toggle = el('button', `btn-sm${it.blocked ? '' : ' on'}`,
        it.blocked ? '启用' : '禁用');
      toggle.title = it.blocked
        ? '清除 Blocked 标记，恢复显示（不删数据）'
        : '加 Blocked 标记临时隐藏，随时可恢复';
      toggle.onclick = () => toggleItem(it, !it.blocked);
      actions.appendChild(toggle);

      const del = el('button', 'btn-sm danger', '删除');
      del.title = '彻底删除注册表键（会自动备份 .reg）';
      del.disabled = it.is_system;
      if (it.is_system) del.title = '系统自带项，不提供删除';
      del.onclick = () => removeItem(it);
      actions.appendChild(del);
    }

    row.appendChild(actions);
    list.appendChild(row);
  }
}

// ---------- 操作 ----------
async function withBusy(fn) {
  if (state.busy) return;
  state.busy = true;
  document.body.classList.add('loading');
  try {
    await fn();
  } finally {
    state.busy = false;
    document.body.classList.remove('loading');
  }
}

async function loadItems() {
  state.items = await invoke('list_items');
  renderScopes();
  renderList();
}

async function toggleItem(item, blocked) {
  await withBusy(async () => {
    try {
      const msg = await invoke('set_item_blocked', { item, blocked });
      toast(msg, 'ok');
      await loadItems();
    } catch (e) {
      toast(String(e), 'err');
    }
  });
}

async function removeItem(item) {
  // 两种模式下的后果不同，确认文案也要跟着变。
  const tail = settings.disableMode === 'delete'
    ? '当前设置是「备份后删除」，所以这条会从列表里消失。\n想保留可恢复的开关，请到设置里改成「标记隐藏」。'
    : '想临时关掉而不是删除，请用「禁用」按钮。';

  const ok = confirm(
    `彻底删除「${item.display_name}」？\n\n` +
    `注册表位置：${item.hive} · ${item.scope}\n` +
    `键名：${item.name}\n\n` +
    `删除前会自动导出一份 .reg 备份到：\n%APPDATA%\\CtxMenuManager\\backup\n\n` +
    tail
  );
  if (!ok) return;

  await withBusy(async () => {
    try {
      const msg = await invoke('delete_item', { item });
      toast(msg, 'ok');
      await loadItems();
    } catch (e) {
      toast(String(e), 'err');
    }
  });
}

// ---------- 经典菜单 ----------
$('btnClassic').onclick = () => withBusy(async () => {
  const enable = !state.classic.enabled;
  try {
    const msg = await invoke('set_classic_menu', { enable });
    toast(msg + '（点击「重启资源管理器」立即生效）', 'ok');
    await loadClassicState();
  } catch (e) {
    toast(String(e), 'err');
  }
});

$('btnRestart').onclick = () => withBusy(async () => {
  const ok = confirm('重启资源管理器会关闭所有已打开的文件夹窗口。\n正在运行的程序不受影响。\n\n继续吗？');
  if (!ok) return;
  try {
    const msg = await invoke('restart_explorer');
    toast(msg, 'ok');
  } catch (e) {
    toast(String(e), 'err');
  }
});

$('btnRefresh').onclick = () => withBusy(async () => {
  await loadItems();
  await loadClassicState();
  toast('已刷新', 'ok');
});

// ---------- 新建 ----------
$('btnNew').onclick = () => {
  const sel = $('newScope');
  sel.innerHTML = '';
  for (const s of SCOPES) {
    const o = document.createElement('option');
    o.value = s.key;
    o.textContent = s.label;
    sel.appendChild(o);
  }
  sel.value = state.scope;
  $('newName').value = '';
  $('newDisplay').value = '';
  $('newCommand').value = '';
  $('modal').hidden = false;
  $('newName').focus();
};

$('btnCancel').onclick = () => { $('modal').hidden = true; };

$('btnCreate').onclick = () => withBusy(async () => {
  const payload = {
    hive: $('newHive').value,
    scope: $('newScope').value,
    name: $('newName').value.trim(),
    display: $('newDisplay').value.trim(),
    command: $('newCommand').value.trim(),
  };
  if (!payload.name || !payload.display || !payload.command) {
    toast('键名、显示文字、命令都要填', 'err');
    return;
  }
  try {
    const msg = await invoke('create_verb', payload);
    $('modal').hidden = true;
    state.scope = payload.scope;
    toast(msg, 'ok');
    await loadItems();
  } catch (e) {
    toast(String(e), 'err');
  }
});

// ---------- 筛选 ----------
$('search').oninput = (e) => {
  state.search = e.target.value;
  renderList();
};
$('onlyEnabled').onchange = (e) => {
  state.onlyEnabled = e.target.checked;
  renderList();
};
$('hideSystem').onchange = (e) => {
  state.hideSystem = e.target.checked;
  renderList();
};

document.addEventListener('keydown', (e) => {
  if (e.key !== 'Escape') return;
  if (!$('settingsModal').hidden) { $('settingsModal').hidden = true; return; }
  if (!$('modal').hidden) $('modal').hidden = true;
});

// ---------- 设置面板 ----------

// 分段控件通用绑定：把当前值对应的按钮标记为 active。
function syncSegmented(containerId, value) {
  for (const b of $(containerId).querySelectorAll('.seg')) {
    b.classList.toggle('active', b.dataset.value === value);
  }
}

function renderThemes() {
  const grid = $('themeGrid');
  grid.innerHTML = '';
  for (const t of THEMES) {
    const card = document.createElement('button');
    card.className = `theme-card${t.key === settings.theme ? ' active' : ''}`;
    card.type = 'button';

    const swatch = document.createElement('div');
    swatch.className = 'theme-swatch';
    for (const c of t.colors) {
      const i = document.createElement('i');
      i.style.background = c;
      swatch.appendChild(i);
    }
    card.appendChild(swatch);

    const name = document.createElement('div');
    name.className = 'theme-name';
    name.textContent = t.name;
    card.appendChild(name);

    card.onclick = () => {
      settings.theme = t.key;
      applyTheme(t.key);
      saveSettings();
      renderThemes(); // 重绘以更新选中态
    };

    grid.appendChild(card);
  }
}

function syncSettingsUI() {
  syncSegmented('segDisableMode', settings.disableMode);
  syncSegmented('segTooltip', settings.tooltip ? 'on' : 'off');
  renderThemes();
}

$('btnSettings').onclick = () => {
  syncSettingsUI();
  $('settingsModal').hidden = false;
};

$('btnSettingsClose').onclick = () => { $('settingsModal').hidden = true; };

// 点遮罩关闭
$('settingsModal').onclick = (e) => {
  const link = e.target.closest('.about-link');
  if (link) {
    invoke('open_url', { url: link.dataset.href }).catch((err) => toast(String(err), 'err'));
    return;
  }
  if (e.target === $('settingsModal')) $('settingsModal').hidden = true;
};

// 禁用方式切换：直接影响列表里每条的操作按钮，需要重绘。
$('segDisableMode').onclick = (e) => {
  const b = e.target.closest('.seg');
  if (!b) return;
  settings.disableMode = b.dataset.value;
  saveSettings();
  syncSegmented('segDisableMode', settings.disableMode);
  renderList();
};

// 悬停说明开关
$('segTooltip').onclick = (e) => {
  const b = e.target.closest('.seg');
  if (!b) return;
  settings.tooltip = b.dataset.value === 'on';
  saveSettings();
  applyTooltipSetting();
  syncSegmented('segTooltip', b.dataset.value);
  if (!settings.tooltip) $('tip').hidden = true;
};

// ---------- 启动 ----------
(async function init() {
  await loadSystemInfo();
  await loadClassicState();
  try {
    await loadItems();
  } catch (e) {
    toast('读取菜单项失败：' + e, 'err');
  }
})();
