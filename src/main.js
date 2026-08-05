const { invoke } = window.__TAURI__.core;
const { open, save, ask } = window.__TAURI__.dialog;

const FILE_FILTERS = [
  { name: "Text", extensions: ["txt"] },
  { name: "Markdown", extensions: ["md"] },
  { name: "All files", extensions: ["*"] },
];

const FONT_MIN = 10;
const FONT_MAX = 48;
const FONT_DEFAULT = 14;
/** Session backup interval (20 minutes). Also saves on blur/close. */
const BACKUP_MS = 20 * 60 * 1000;

/** @typedef {{ id: number, title: string, path: string|null, content: string, saved: string, threadBase: string|null, snapN: number, wrap: boolean }} Tab */

let nextId = 1;
/** @type {Tab[]} */
let tabs = [];
/** @type {number} */
let activeId = 0;
/** @type {'light'|'dark'|'system'} */
let themePref = "light";
let fontSize = FONT_DEFAULT;
/** @type {MediaQueryList|null} */
let systemDarkMq = null;
/** @type {string} */
let dataDir = "";
/** @type {string} */
let notesDir = "";
let backupTimer = 0;
let backupBusy = false;

const el = {
  tabs: document.getElementById("tabs"),
  editor: document.getElementById("editor"),
  status: document.getElementById("status"),
  snapInfo: document.getElementById("snap-info"),
  wrap: document.getElementById("btn-wrap"),
  theme: document.getElementById("theme-select"),
};

function active() {
  return tabs.find((t) => t.id === activeId);
}

function isDirty(tab) {
  return tab.content !== tab.saved;
}

function fileName(path) {
  return path.replace(/^.*[\\/]/, "");
}

function displayTitle(tab) {
  const name = tab.path ? fileName(tab.path) : tab.title || "Untitled";
  return isDirty(tab) ? `${name} *` : name;
}

function stemFromPath(path) {
  const file = fileName(path);
  const i = file.lastIndexOf(".");
  return i > 0 ? file.slice(0, i) : file;
}

function extFromPath(path, fallback = ".txt") {
  const file = fileName(path);
  const i = file.lastIndexOf(".");
  return i > 0 ? file.slice(i) : fallback;
}

function dirFromPath(path) {
  const i = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return i >= 0 ? path.slice(0, i) : ".";
}

function joinPath(dir, file) {
  const sep = dir.includes("\\") ? "\\" : "/";
  return `${dir}${sep}${file}`;
}

/** Append .txt only when the chosen name has no extension. */
function ensureDefaultExt(path) {
  const file = fileName(path);
  return file.includes(".") ? path : `${path}.txt`;
}

function defaultThreadBase() {
  const d = new Date();
  const p = (n) => String(n).padStart(2, "0");
  return `note_${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}_${p(d.getHours())}${p(d.getMinutes())}${p(d.getSeconds())}`;
}

function defaultSavePath(tab) {
  const name = `${tab.threadBase || defaultThreadBase()}.txt`;
  return joinPath(notesDir || dataDir, name);
}

function createTab(partial = {}) {
  const id = partial.id != null ? partial.id : nextId++;
  if (id >= nextId) nextId = id + 1;
  const tab = {
    title: "Untitled",
    path: null,
    content: "",
    saved: "",
    threadBase: null,
    snapN: 0,
    wrap: true,
    ...partial,
    id,
  };
  tabs.push(tab);
  return tab;
}

function setStatus(msg) {
  el.status.textContent = msg;
}

function applyTheme() {
  const resolved =
    themePref === "system"
      ? systemDarkMq?.matches
        ? "dark"
        : "light"
      : themePref;
  document.documentElement.setAttribute("data-theme", resolved);
}

function loadSettings() {
  const t = localStorage.getItem("theme");
  if (t === "light" || t === "dark" || t === "system") themePref = t;
  else themePref = "light";

  const fs = Number(localStorage.getItem("fontSize"));
  fontSize =
    Number.isFinite(fs) && fs >= FONT_MIN && fs <= FONT_MAX ? fs : FONT_DEFAULT;

  el.theme.value = themePref;
  document.documentElement.style.setProperty(
    "--editor-font-size",
    `${fontSize}px`,
  );
  applyTheme();
}

function setTheme(pref) {
  themePref = pref;
  localStorage.setItem("theme", pref);
  el.theme.value = pref;
  applyTheme();
}

function setFontSize(size) {
  fontSize = Math.min(FONT_MAX, Math.max(FONT_MIN, size));
  localStorage.setItem("fontSize", String(fontSize));
  document.documentElement.style.setProperty(
    "--editor-font-size",
    `${fontSize}px`,
  );
  const tab = active();
  const path = tab?.path || "(unsaved)";
  setStatus(`${displayTitle(tab)}  —  ${path}  ·  ${fontSize}px`);
}

function sessionPath() {
  return joinPath(dataDir, "session.json");
}

function sessionPayload() {
  const cur = active();
  if (cur) cur.content = el.editor.value;
  return {
    activeId,
    nextId,
    tabs: tabs.map((t) => ({
      id: t.id,
      title: t.title,
      path: t.path,
      content: t.content,
      saved: t.saved,
      threadBase: t.threadBase,
      snapN: t.snapN,
      wrap: t.wrap,
    })),
  };
}

async function backupSession(silent = true) {
  if (!dataDir || backupBusy) return;
  backupBusy = true;
  try {
    await invoke("write_text", {
      path: sessionPath(),
      contents: JSON.stringify(sessionPayload(), null, 2),
    });
    if (!silent) {
      const tab = active();
      const path = tab?.path || "(unsaved)";
      setStatus(`${displayTitle(tab)}  —  ${path}  ·  backup ok`);
    }
  } catch (err) {
    console.error("session backup failed", err);
  } finally {
    backupBusy = false;
  }
}

async function restoreSession() {
  try {
    const raw = await invoke("read_text", { path: sessionPath() });
    const data = JSON.parse(raw);
    if (!data?.tabs?.length) return false;

    tabs = [];
    nextId = Number(data.nextId) || 1;
    for (const t of data.tabs) {
      createTab({
        id: t.id,
        title: t.title || "Untitled",
        path: t.path ?? null,
        content: t.content ?? "",
        saved: t.saved ?? "",
        threadBase: t.threadBase ?? null,
        snapN: t.snapN ?? 0,
        wrap: t.wrap !== false,
      });
    }
    nextId = Math.max(nextId, ...tabs.map((t) => t.id + 1));
    activeId = tabs.some((t) => t.id === data.activeId)
      ? data.activeId
      : tabs[0].id;
    renderTabs();
    syncEditorFromTab();
    return true;
  } catch {
    return false;
  }
}

function syncEditorFromTab() {
  const tab = active();
  if (!tab) return;
  el.editor.value = tab.content;
  el.editor.classList.toggle("wrap", tab.wrap);
  el.editor.classList.toggle("no-wrap", !tab.wrap);
  el.wrap.setAttribute("aria-pressed", String(tab.wrap));
  el.snapInfo.textContent =
    tab.snapN > 0
      ? `thread: ${tab.threadBase || "—"}  ·  next: v${tab.snapN + 1}`
      : tab.threadBase
        ? `thread: ${tab.threadBase}  ·  next: v1`
        : "no snapshots yet";
  const path = tab.path || "(unsaved)";
  setStatus(`${displayTitle(tab)}  —  ${path}  ·  ${fontSize}px`);
}

function renderTabs() {
  el.tabs.replaceChildren(
    ...tabs.map((tab) => {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = `tab${tab.id === activeId ? " active" : ""}`;
      btn.title = tab.path || tab.title;
      btn.tabIndex = -1;

      const title = document.createElement("span");
      title.className = "title";
      title.textContent = displayTitle(tab);
      btn.append(title);

      const close = document.createElement("span");
      close.className = "close";
      close.textContent = "×";
      close.title = "Close";
      close.tabIndex = -1;
      close.addEventListener("click", (e) => {
        e.stopPropagation();
        closeTab(tab.id);
      });
      btn.append(close);

      btn.addEventListener("click", () => switchTab(tab.id));
      return btn;
    }),
  );
}

function switchTab(id) {
  const cur = active();
  if (cur) cur.content = el.editor.value;
  activeId = id;
  renderTabs();
  syncEditorFromTab();
  el.editor.focus();
  backupSession();
}

async function confirmDiscard(tab) {
  if (!isDirty(tab)) return true;
  return ask(`Discard changes to "${displayTitle(tab)}"?`, {
    title: "Unsaved changes",
    kind: "warning",
  });
}

async function newTab() {
  const tab = createTab();
  activeId = tab.id;
  renderTabs();
  syncEditorFromTab();
  el.editor.focus();
  backupSession();
}

async function closeTab(id) {
  const tab = tabs.find((t) => t.id === id);
  if (!tab) return;
  if (tab.id === activeId) tab.content = el.editor.value;
  if (!(await confirmDiscard(tab))) return;

  const idx = tabs.findIndex((t) => t.id === id);
  tabs.splice(idx, 1);
  if (tabs.length === 0) {
    await newTab();
    return;
  }
  if (activeId === id) {
    activeId = tabs[Math.min(idx, tabs.length - 1)].id;
  }
  renderTabs();
  syncEditorFromTab();
  backupSession();
}

async function openFile() {
  const selected = await open({
    multiple: false,
    filters: FILE_FILTERS,
    defaultPath: notesDir || undefined,
  });
  if (!selected) return;

  const content = await invoke("read_text", { path: selected });
  const base = stemFromPath(selected).replace(/_v\d+$/i, "");
  const snapN = await invoke("latest_snap_n", {
    dir: dirFromPath(selected),
    base,
  });

  const existing = tabs.find((t) => t.path === selected);
  if (existing) {
    existing.content = content;
    existing.saved = content;
    existing.threadBase = base;
    existing.snapN = snapN;
    switchTab(existing.id);
    return;
  }

  const tab = createTab({
    title: fileName(selected),
    path: selected,
    content,
    saved: content,
    threadBase: base,
    snapN,
  });
  activeId = tab.id;
  renderTabs();
  syncEditorFromTab();
  backupSession();
}

async function saveTab(tab, { saveAs = false } = {}) {
  tab.content = el.editor.value;
  let path = tab.path;

  if (!path || saveAs) {
    path = await save({
      defaultPath: path || defaultSavePath(tab),
      filters: FILE_FILTERS,
    });
    if (!path) return false;
    path = ensureDefaultExt(path);
  }

  await invoke("write_text", { path, contents: tab.content });
  tab.path = path;
  tab.title = fileName(path);
  tab.saved = tab.content;
  if (!tab.threadBase) {
    tab.threadBase = stemFromPath(path).replace(/_v\d+$/i, "");
    tab.snapN = await invoke("latest_snap_n", {
      dir: dirFromPath(path),
      base: tab.threadBase,
    });
  }
  renderTabs();
  syncEditorFromTab();
  setStatus(`Saved ${path}`);
  backupSession();
  return true;
}

async function saveSnapshot() {
  const tab = active();
  if (!tab) return;
  tab.content = el.editor.value;

  if (!tab.content.trim()) {
    setStatus("Nothing to snapshot");
    return;
  }

  if (!tab.threadBase) {
    if (tab.path) {
      tab.threadBase = stemFromPath(tab.path).replace(/_v\d+$/i, "");
    } else {
      tab.threadBase = defaultThreadBase();
    }
  }

  const ext = tab.path ? extFromPath(tab.path) : ".txt";

  let dir;
  if (tab.path) {
    dir = dirFromPath(tab.path);
  } else {
    const pick = await save({
      defaultPath: joinPath(notesDir || dataDir, `${tab.threadBase}_v1.txt`),
      filters: FILE_FILTERS,
      title: "Choose folder for this snapshot thread",
    });
    if (!pick) return;
    dir = dirFromPath(pick);
    const pickedBase = stemFromPath(ensureDefaultExt(pick)).replace(
      /_v\d+$/i,
      "",
    );
    if (pickedBase) tab.threadBase = pickedBase;
  }

  const n = await invoke("next_snap_n", { dir, base: tab.threadBase });
  const snapPath = joinPath(dir, `${tab.threadBase}_v${n}${ext}`);
  await invoke("write_text", { path: snapPath, contents: tab.content });
  tab.snapN = n;

  if (!tab.path) {
    tab.path = joinPath(dir, `${tab.threadBase}${ext}`);
    await invoke("write_text", { path: tab.path, contents: tab.content });
    tab.saved = tab.content;
    tab.title = `${tab.threadBase}${ext}`;
  }

  renderTabs();
  syncEditorFromTab();
  setStatus(`Snapshot saved: ${tab.threadBase}_v${n}${ext}`);
  backupSession();
}

function toggleWrap() {
  const tab = active();
  if (!tab) return;
  tab.wrap = !tab.wrap;
  syncEditorFromTab();
}

function insertTabIndent() {
  const start = el.editor.selectionStart;
  const end = el.editor.selectionEnd;
  const v = el.editor.value;
  el.editor.value = `${v.slice(0, start)}\t${v.slice(end)}`;
  el.editor.selectionStart = el.editor.selectionEnd = start + 1;
  const tab = active();
  if (tab) {
    tab.content = el.editor.value;
    renderTabs();
  }
}

function bind() {
  document.getElementById("btn-new").addEventListener("click", () => newTab());
  document.getElementById("btn-open").addEventListener("click", () => openFile());
  document.getElementById("btn-save").addEventListener("click", () =>
    saveTab(active()),
  );
  document.getElementById("btn-save-as").addEventListener("click", () =>
    saveTab(active(), { saveAs: true }),
  );
  document.getElementById("btn-snap").addEventListener("click", () =>
    saveSnapshot(),
  );
  el.wrap.addEventListener("click", () => toggleWrap());
  el.theme.addEventListener("change", () => setTheme(el.theme.value));

  systemDarkMq = window.matchMedia("(prefers-color-scheme: dark)");
  systemDarkMq.addEventListener("change", () => {
    if (themePref === "system") applyTheme();
  });

  el.editor.addEventListener("input", () => {
    const tab = active();
    if (!tab) return;
    tab.content = el.editor.value;
    renderTabs();
    const path = tab.path || "(unsaved)";
    setStatus(`${displayTitle(tab)}  —  ${path}  ·  ${fontSize}px`);
  });

  el.editor.addEventListener("keydown", (e) => {
    if (e.key !== "Tab" || e.ctrlKey || e.metaKey || e.altKey) return;
    e.preventDefault();
    insertTabIndent();
  });

  el.editor.addEventListener(
    "wheel",
    (e) => {
      if (!(e.ctrlKey || e.metaKey)) return;
      e.preventDefault();
      setFontSize(fontSize + (e.deltaY < 0 ? 1 : -1));
    },
    { passive: false },
  );

  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "hidden") backupSession();
  });
  window.addEventListener("pagehide", () => backupSession());
  window.addEventListener("beforeunload", () => backupSession());

  window.addEventListener("keydown", (e) => {
    const mod = e.ctrlKey || e.metaKey;
    if (!mod) return;
    const key = e.key.toLowerCase();
    if (key === "n") {
      e.preventDefault();
      newTab();
    } else if (key === "o") {
      e.preventDefault();
      openFile();
    } else if (key === "s") {
      e.preventDefault();
      if (e.shiftKey) saveTab(active(), { saveAs: true });
      else saveTab(active());
    } else if (key === "w") {
      e.preventDefault();
      closeTab(activeId);
    } else if (key === "=" || key === "+") {
      e.preventDefault();
      setFontSize(fontSize + 1);
    } else if (key === "-" || key === "_") {
      e.preventDefault();
      setFontSize(fontSize - 1);
    } else if (key === "0") {
      e.preventDefault();
      setFontSize(FONT_DEFAULT);
    }
  });
}

window.addEventListener("DOMContentLoaded", async () => {
  loadSettings();
  bind();

  try {
    dataDir = await invoke("app_data_dir");
    notesDir = joinPath(dataDir, "notes");
  } catch (err) {
    console.error("app_data_dir failed", err);
  }

  const restored = await restoreSession();
  if (!restored) await newTab();

  backupTimer = window.setInterval(() => backupSession(), BACKUP_MS);
  el.editor.focus();
});
