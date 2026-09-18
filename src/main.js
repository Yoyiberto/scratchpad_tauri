import {
  findWordAtPosition,
  findParagraphAtPosition,
  applyCorrectionToText,
  createCorrectionRecord,
  correctionsToCsv,
} from "./transcription.js";

const tauriCore =
  typeof window !== "undefined" && window.__TAURI__?.core
    ? window.__TAURI__.core
    : {};
const tauriDialog =
  typeof window !== "undefined" && window.__TAURI__?.dialog
    ? window.__TAURI__.dialog
    : {};
const invoke = tauriCore.invoke || (async () => {});
const { open, save, ask } = tauriDialog;

const FILE_FILTERS = [
  { name: "Text", extensions: ["txt"] },
  { name: "Markdown", extensions: ["md"] },
  { name: "All files", extensions: ["*"] },
];

const FONT_MIN = 10;
const FONT_MAX = 48;
const FONT_DEFAULT = 14;
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
let systemDarkMq = null;
let dataDir = "";
let notesDir = "";
let backupTimer = 0;
let backupBusy = false;

// Transcription feature state
/** @type {Array<{ id: number, original: string, corrected: string, context: string, file: string, timestamp: string }>} */
let correctionsList = [];
let triggerMode = "ctrl_dblclick"; // 'ctrl_dblclick' or 'dblclick'
let activeCorrectionTarget = null;

const el =
  typeof document !== "undefined"
    ? {
        tabs: document.getElementById("tabs"),
        editor: document.getElementById("editor"),
        status: document.getElementById("status"),
        headerDocTitle: document.getElementById("header-doc-title"),
        headerDocSub: document.getElementById("header-doc-sub"),
        btnOpen: document.getElementById("btn-open"),
        btnNew: document.getElementById("btn-new"),
        btnSave: document.getElementById("btn-save"),
        btnSnap: document.getElementById("btn-snap"),
        btnCorrections: document.getElementById("btn-corrections"),
        corrBadge: document.getElementById("corr-badge"),
        btnMenu: document.getElementById("btn-menu"),
        appMenu: document.getElementById("app-menu"),
        menuSaveAs: document.getElementById("menu-save-as"),
        menuWrap: document.getElementById("menu-wrap"),
        menuWrapText: document.getElementById("menu-wrap-text"),
        menuDcToggle: document.getElementById("menu-dc-toggle"),
        menuDcText: document.getElementById("menu-dc-text"),
        menuViewCorrections: document.getElementById("menu-view-corrections"),
        themeSelect: document.getElementById("theme-select"),
        btnFontDec: document.getElementById("btn-font-dec"),
        btnFontInc: document.getElementById("btn-font-inc"),
        fontSizeLabel: document.getElementById("font-size-label"),
        // Correction Modal
        corrModal: document.getElementById("corr-modal"),
        corrOrigWord: document.getElementById("corr-orig-word"),
        corrNewWord: document.getElementById("corr-new-word"),
        corrContext: document.getElementById("corr-context"),
        corrBtnSave: document.getElementById("corr-btn-save"),
        corrBtnCancel: document.getElementById("corr-btn-cancel"),
        corrCloseBtn: document.getElementById("corr-close-btn"),
        // History Modal
        historyModal: document.getElementById("history-modal"),
        historyCloseBtn: document.getElementById("history-close-btn"),
        historySummary: document.getElementById("history-summary"),
        correctionsTbody: document.getElementById("corrections-tbody"),
        btnExportJson: document.getElementById("btn-export-json"),
        btnExportCsv: document.getElementById("btn-export-csv"),
        btnClearCorrections: document.getElementById("btn-clear-corrections"),
        // Window Controls
        headerbar: document.getElementById("headerbar"),
        winBtnMin: document.getElementById("win-btn-min"),
        winBtnMax: document.getElementById("win-btn-max"),
        winBtnClose: document.getElementById("win-btn-close"),
      }
    : {};

function active() {
  return tabs.find((t) => t.id === activeId);
}

function isDirty(tab) {
  if (!tab) return false;
  return tab.content !== tab.saved;
}

function fileName(path) {
  return path ? path.replace(/^.*[\\/]/, "") : "";
}

function displayTitle(tab) {
  if (!tab) return "Untitled";
  return tab.path ? fileName(tab.path) : tab.title || "Untitled";
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
  if (!dir) return file;
  if (dir.endsWith("/") || dir.endsWith("\\")) {
    return `${dir}${file}`;
  }
  const sep = dir.includes("\\") ? "\\" : "/";
  return `${dir}${sep}${file}`;
}

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
  if (el.status) el.status.textContent = msg;
}

function updateHeaderDocInfo() {
  const cur = active();
  if (!cur || !el.headerDocTitle) return;
  el.headerDocTitle.textContent = displayTitle(cur);
  const dirty = isDirty(cur);
  if (dirty) {
    el.headerDocSub.textContent = "Draft (Unsaved)";
    el.headerDocSub.style.color = "var(--accent)";
  } else {
    el.headerDocSub.textContent = cur.snapN > 0 ? `Snapshot v${cur.snapN}` : (cur.path ? "Saved" : "Clean");
    el.headerDocSub.style.color = "var(--text-sub)";
  }
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

  const tm = localStorage.getItem("triggerMode");
  if (tm === "dblclick" || tm === "ctrl_dblclick") triggerMode = tm;

  if (el.themeSelect) el.themeSelect.value = themePref;
  if (el.fontSizeLabel) el.fontSizeLabel.textContent = `${fontSize}px`;
  if (el.menuDcText) {
    el.menuDcText.textContent = triggerMode === "ctrl_dblclick" ? "Trigger: Ctrl+Double-Click" : "Trigger: Double-Click";
  }

  document.documentElement.style.setProperty(
    "--editor-font-size",
    `${fontSize}px`,
  );
  applyTheme();
}

function setTheme(pref) {
  themePref = pref;
  localStorage.setItem("theme", pref);
  if (el.themeSelect) el.themeSelect.value = pref;
  applyTheme();
}

function setFontSize(size) {
  fontSize = Math.min(FONT_MAX, Math.max(FONT_MIN, size));
  localStorage.setItem("fontSize", String(fontSize));
  document.documentElement.style.setProperty(
    "--editor-font-size",
    `${fontSize}px`,
  );
  if (el.fontSizeLabel) el.fontSizeLabel.textContent = `${fontSize}px`;
  const tab = active();
  const path = tab?.path || "(unsaved)";
  setStatus(`${displayTitle(tab)}  —  ${path}  ·  ${fontSize}px`);
}

function toggleTriggerMode() {
  triggerMode = triggerMode === "ctrl_dblclick" ? "dblclick" : "ctrl_dblclick";
  localStorage.setItem("triggerMode", triggerMode);
  if (el.menuDcText) {
    el.menuDcText.textContent = triggerMode === "ctrl_dblclick" ? "Trigger: Ctrl+Double-Click" : "Trigger: Double-Click";
  }
  setStatus(`Transcription trigger mode: ${triggerMode === "ctrl_dblclick" ? "Ctrl + Double-Click" : "Double-Click"}`);
}

function sessionPath() {
  return joinPath(dataDir, "session.json");
}

function sessionPayload() {
  const cur = active();
  if (cur && el.editor) cur.content = el.editor.value;
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
  if (!tab || !el.editor) return;
  el.editor.value = tab.content;
  el.editor.classList.toggle("wrap", tab.wrap);
  el.editor.classList.toggle("no-wrap", !tab.wrap);

  if (el.menuWrapText) {
    el.menuWrapText.textContent = `Word Wrap: ${tab.wrap ? "On" : "Off"}`;
  }

  updateHeaderDocInfo();
  const path = tab.path || "(unsaved)";
  setStatus(`${displayTitle(tab)}  —  ${path}  ·  ${fontSize}px`);
}

function renderTabs() {
  if (!el.tabs) return;
  el.tabs.replaceChildren(
    ...tabs.map((tab) => {
      const btn = document.createElement("button");
      btn.type = "button";
      const dirty = isDirty(tab);
      btn.className = `tab${tab.id === activeId ? " active" : ""}${dirty ? " dirty" : ""}`;
      btn.title = tab.path || tab.title;
      btn.tabIndex = -1;

      const dot = document.createElement("span");
      dot.className = "tab-dot";
      btn.append(dot);

      const title = document.createElement("span");
      title.className = "title";
      title.textContent = displayTitle(tab);
      btn.append(title);

      const close = document.createElement("span");
      close.className = "close";
      close.textContent = "×";
      close.title = "Close tab";
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
  if (cur && el.editor) cur.content = el.editor.value;
  activeId = id;
  renderTabs();
  syncEditorFromTab();
  if (el.editor) el.editor.focus();
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
  if (el.editor) el.editor.focus();
  backupSession();
}

async function closeTab(id) {
  const tab = tabs.find((t) => t.id === id);
  if (!tab) return;
  if (tab.id === activeId && el.editor) tab.content = el.editor.value;
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
  if (!tab || !el.editor) return false;
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
  if (!tab || !el.editor) return;
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

/* ==================== Transcription Correction Features ==================== */

async function loadCorrections() {
  try {
    const list = await invoke("load_corrections");
    if (Array.isArray(list)) {
      correctionsList = list;
    }
  } catch {
    const raw = localStorage.getItem("plain_notepad_corrections");
    if (raw) {
      try {
        correctionsList = JSON.parse(raw);
      } catch {
        correctionsList = [];
      }
    }
  }
  updateCorrectionsBadge();
}

function updateCorrectionsBadge() {
  if (el.corrBadge) {
    el.corrBadge.textContent = String(correctionsList.length);
  }
  if (el.historySummary) {
    el.historySummary.textContent = `${correctionsList.length} corrections recorded`;
  }
}

function triggerCorrectionAtPosition(position) {
  if (!el.editor) return;
  const text = el.editor.value;
  const wordInfo = findWordAtPosition(text, position);
  if (!wordInfo || !wordInfo.word) return;

  const paragraph = findParagraphAtPosition(text, position);
  activeCorrectionTarget = {
    ...wordInfo,
    paragraph,
  };

  showCorrectionModal(activeCorrectionTarget);
}

function showCorrectionModal(target) {
  if (!el.corrModal) return;
  el.corrOrigWord.textContent = target.word;
  el.corrNewWord.value = target.word;

  // Highlight word inside paragraph preview
  const safeP = target.paragraph
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
  const regex = new RegExp(`\\b(${escapeRegex(target.word)})\\b`, "i");
  el.corrContext.innerHTML = safeP.replace(
    regex,
    '<span class="context-highlight">$1</span>',
  );

  el.corrModal.classList.remove("hidden");
  el.corrNewWord.focus();
  el.corrNewWord.select();
}

function escapeRegex(str) {
  return str.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function closeCorrectionModal() {
  if (!el.corrModal) return;
  el.corrModal.classList.add("hidden");
  activeCorrectionTarget = null;
  if (el.editor) el.editor.focus();
}

async function commitCorrection() {
  if (!activeCorrectionTarget || !el.corrNewWord) return;
  const newWord = el.corrNewWord.value.trim();
  if (!newWord) {
    closeCorrectionModal();
    return;
  }

  const oldWord = activeCorrectionTarget.word;
  const currentText = el.editor.value;
  const updatedText = applyCorrectionToText(
    currentText,
    activeCorrectionTarget.startIndex,
    activeCorrectionTarget.endIndex,
    newWord,
  );

  el.editor.value = updatedText;
  const tab = active();
  if (tab) {
    tab.content = updatedText;
    renderTabs();
    updateHeaderDocInfo();
  }

  // Create record
  const record = createCorrectionRecord({
    original: oldWord,
    corrected: newWord,
    context: activeCorrectionTarget.paragraph,
    file: tab ? displayTitle(tab) : "Untitled",
  });

  correctionsList.unshift(record);
  updateCorrectionsBadge();

  // Save to backend and localStorage
  try {
    await invoke("save_correction", { correction: record });
  } catch (err) {
    console.error("Failed to save correction to backend", err);
  }
  localStorage.setItem(
    "plain_notepad_corrections",
    JSON.stringify(correctionsList),
  );

  closeCorrectionModal();
  setStatus(`Correction saved: "${oldWord}" → "${newWord}"`);
  backupSession();
}

function renderCorrectionsTable() {
  if (!el.correctionsTbody) return;
  el.correctionsTbody.innerHTML = "";

  if (correctionsList.length === 0) {
    const tr = document.createElement("tr");
    tr.innerHTML = `<td colspan="4" style="text-align:center; padding: 20px; color: var(--text-muted);">No corrections saved yet.</td>`;
    el.correctionsTbody.append(tr);
    return;
  }

  for (const item of correctionsList) {
    const tr = document.createElement("tr");
    const d = new Date(item.timestamp);
    const dateStr = `${d.toLocaleDateString()} ${d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}`;

    tr.innerHTML = `
      <td><span class="tag-orig">${escapeHtml(item.original)}</span></td>
      <td><span class="tag-corr">${escapeHtml(item.corrected)}</span></td>
      <td><div class="tag-context" title="${escapeHtml(item.context)}">${escapeHtml(item.context)}</div></td>
      <td style="color: var(--text-muted); font-size: 11px;">${dateStr}</td>
    `;
    el.correctionsTbody.append(tr);
  }
}

function escapeHtml(str) {
  return String(str || "")
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function openHistoryModal() {
  if (!el.historyModal) return;
  updateCorrectionsBadge();
  renderCorrectionsTable();
  el.historyModal.classList.remove("hidden");
}

function closeHistoryModal() {
  if (!el.historyModal) return;
  el.historyModal.classList.add("hidden");
  if (el.editor) el.editor.focus();
}

async function exportCorrectionsJson() {
  const jsonStr = JSON.stringify(correctionsList, null, 2);
  const cur = active();
  const baseName = cur ? stemFromPath(cur.title) : "notepad";
  const defaultP = joinPath(notesDir || dataDir, `${baseName}_corrections.json`);
  const pick = await save({
    defaultPath: defaultP,
    filters: [{ name: "JSON", extensions: ["json"] }],
  });
  if (pick) {
    await invoke("write_text", { path: pick, contents: jsonStr });
    setStatus(`Exported corrections JSON: ${pick}`);
  }
}

async function exportCorrectionsCsv() {
  const csvStr = correctionsToCsv(correctionsList);
  const cur = active();
  const baseName = cur ? stemFromPath(cur.title) : "notepad";
  const defaultP = joinPath(notesDir || dataDir, `${baseName}_corrections.csv`);
  const pick = await save({
    defaultPath: defaultP,
    filters: [{ name: "CSV", extensions: ["csv"] }],
  });
  if (pick) {
    await invoke("write_text", { path: pick, contents: csvStr });
    setStatus(`Exported corrections CSV: ${pick}`);
  }
}

async function clearAllCorrections() {
  if (correctionsList.length === 0) return;
  const ok = await ask("Are you sure you want to clear all recorded corrections?", {
    title: "Clear Corrections",
    kind: "warning",
  });
  if (!ok) return;

  correctionsList = [];
  try {
    await invoke("clear_corrections");
  } catch {}
  localStorage.removeItem("plain_notepad_corrections");
  updateCorrectionsBadge();
  renderCorrectionsTable();
  setStatus("All corrections cleared");
}

/* ==================== Event Binding ==================== */

function bind() {
  // Top bar buttons
  el.btnOpen?.addEventListener("click", () => openFile());
  el.btnNew?.addEventListener("click", () => newTab());
  el.btnSave?.addEventListener("click", () => saveTab(active()));
  el.btnSnap?.addEventListener("click", () => saveSnapshot());
  el.btnCorrections?.addEventListener("click", () => openHistoryModal());

  // Window Controls (GNOME CSD)
  el.winBtnMin?.addEventListener("click", () => invoke("win_minimize"));
  el.winBtnMax?.addEventListener("click", () => invoke("win_toggle_maximize"));
  el.winBtnClose?.addEventListener("click", () => invoke("win_close"));
  el.headerbar?.addEventListener("dblclick", (e) => {
    if (e.target.closest("button") || e.target.closest("select") || e.target.closest("input")) return;
    invoke("win_toggle_maximize");
  });

  // Menu dropdown
  el.btnMenu?.addEventListener("click", (e) => {
    e.stopPropagation();
    el.appMenu?.classList.toggle("hidden");
  });

  document.addEventListener("click", (e) => {
    if (!el.appMenu?.contains(e.target) && e.target !== el.btnMenu) {
      el.appMenu?.classList.add("hidden");
    }
  });

  el.menuSaveAs?.addEventListener("click", () => {
    el.appMenu?.classList.add("hidden");
    saveTab(active(), { saveAs: true });
  });

  el.menuWrap?.addEventListener("click", () => {
    toggleWrap();
    el.appMenu?.classList.add("hidden");
  });

  el.menuDcToggle?.addEventListener("click", () => {
    toggleTriggerMode();
    el.appMenu?.classList.add("hidden");
  });

  el.menuViewCorrections?.addEventListener("click", () => {
    el.appMenu?.classList.add("hidden");
    openHistoryModal();
  });

  el.themeSelect?.addEventListener("change", () => setTheme(el.themeSelect.value));
  el.btnFontDec?.addEventListener("click", () => setFontSize(fontSize - 1));
  el.btnFontInc?.addEventListener("click", () => setFontSize(fontSize + 1));

  // Correction Modal
  el.corrBtnCancel?.addEventListener("click", () => closeCorrectionModal());
  el.corrCloseBtn?.addEventListener("click", () => closeCorrectionModal());
  el.corrBtnSave?.addEventListener("click", () => commitCorrection());
  el.corrNewWord?.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      commitCorrection();
    } else if (e.key === "Escape") {
      e.preventDefault();
      closeCorrectionModal();
    }
  });

  // History Modal
  el.historyCloseBtn?.addEventListener("click", () => closeHistoryModal());
  el.btnExportJson?.addEventListener("click", () => exportCorrectionsJson());
  el.btnExportCsv?.addEventListener("click", () => exportCorrectionsCsv());
  el.btnClearCorrections?.addEventListener("click", () => clearAllCorrections());

  // Editor transcription trigger
  el.editor?.addEventListener("dblclick", (e) => {
    const shouldTrigger =
      triggerMode === "dblclick" ||
      (triggerMode === "ctrl_dblclick" && (e.ctrlKey || e.metaKey));
    if (!shouldTrigger) return;

    const pos = el.editor.selectionStart;
    triggerCorrectionAtPosition(pos);
  });

  el.editor?.addEventListener("input", () => {
    const tab = active();
    if (!tab) return;
    tab.content = el.editor.value;
    renderTabs();
    updateHeaderDocInfo();
  });

  el.editor?.addEventListener("keydown", (e) => {
    if (e.key === "Tab" && !e.ctrlKey && !e.metaKey && !e.altKey) {
      e.preventDefault();
      const start = el.editor.selectionStart;
      const end = el.editor.selectionEnd;
      const v = el.editor.value;
      el.editor.value = `${v.slice(0, start)}\t${v.slice(end)}`;
      el.editor.selectionStart = el.editor.selectionEnd = start + 1;
      const tab = active();
      if (tab) {
        tab.content = el.editor.value;
        renderTabs();
        updateHeaderDocInfo();
      }
    }
  });

  // Global shortcuts
  window.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      if (!el.corrModal?.classList.contains("hidden")) {
        closeCorrectionModal();
        return;
      }
      if (!el.historyModal?.classList.contains("hidden")) {
        closeHistoryModal();
        return;
      }
      if (!el.appMenu?.classList.contains("hidden")) {
        el.appMenu?.classList.add("hidden");
        return;
      }
    }

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
    } else if (key === "e" || key === "k") {
      // Shortcut to trigger correction on selected word or word at cursor
      e.preventDefault();
      if (el.editor) {
        triggerCorrectionAtPosition(el.editor.selectionStart);
      }
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

  systemDarkMq = window.matchMedia("(prefers-color-scheme: dark)");
  systemDarkMq.addEventListener("change", () => {
    if (themePref === "system") applyTheme();
  });

  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "hidden") backupSession();
  });
  window.addEventListener("pagehide", () => backupSession());
  window.addEventListener("beforeunload", () => backupSession());
}

if (typeof window !== "undefined") {
  window.addEventListener("DOMContentLoaded", async () => {
    loadSettings();
    bind();

    try {
      dataDir = await invoke("app_data_dir");
      notesDir = joinPath(dataDir, "notes");
    } catch (err) {
      console.error("app_data_dir failed", err);
    }

    await loadCorrections();

    const restored = await restoreSession();
    if (!restored) await newTab();

    let testScreen = "";
    try {
      testScreen = (await invoke("get_test_screen")) || localStorage.getItem("__test_screen") || "";
    } catch {}

    if (testScreen === "corr") {
      setTimeout(() => {
        const text = el.editor ? el.editor.value : "";
        const idx = text.indexOf("quitar");
        if (idx >= 0) triggerCorrectionAtPosition(idx + 2);
      }, 350);
    } else if (testScreen === "history") {
      setTimeout(() => {
        openHistoryModal();
      }, 350);
    }

    backupTimer = window.setInterval(() => backupSession(), BACKUP_MS);
    el.editor?.focus();
  });
}

export {
  fileName,
  stemFromPath,
  extFromPath,
  dirFromPath,
  joinPath,
  ensureDefaultExt,
  defaultThreadBase,
  displayTitle,
  isDirty,
  createTab,
  tabs,
  correctionsList,
  triggerMode,
  FILE_FILTERS,
  FONT_MIN,
  FONT_MAX,
  FONT_DEFAULT,
};
