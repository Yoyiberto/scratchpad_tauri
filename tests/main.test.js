import test from "node:test";
import assert from "node:assert/strict";
import {
  fileName,
  stemFromPath,
  extFromPath,
  dirFromPath,
  joinPath,
  ensureDefaultExt,
  displayTitle,
  isDirty,
  createTab,
  defaultThreadBase,
  handleWheelZoom,
  setFontSize,
  getFontSize,
  FONT_MIN,
  FONT_MAX,
  FONT_DEFAULT,
  setTheme,
  resolveCurrentTheme,
  getSystemTheme,
} from "../src/main.js";

test("fileName extracts file name correctly for both Linux and Windows paths", () => {
  assert.equal(fileName("/home/user/documents/note.txt"), "note.txt");
  assert.equal(fileName("C:\\Users\\User\\Documents\\note.txt"), "note.txt");
  assert.equal(fileName("plain_file.md"), "plain_file.md");
});

test("stemFromPath removes extension correctly", () => {
  assert.equal(stemFromPath("/home/user/note.txt"), "note");
  assert.equal(stemFromPath("C:\\Users\\User\\snapshot_v1.txt"), "snapshot_v1");
  assert.equal(stemFromPath("archive.tar.gz"), "archive.tar");
  assert.equal(stemFromPath("noextension"), "noextension");
});

test("extFromPath extracts extension with fallback", () => {
  assert.equal(extFromPath("/home/user/note.txt"), ".txt");
  assert.equal(extFromPath("/home/user/note.md"), ".md");
  assert.equal(extFromPath("/home/user/note"), ".txt");
  assert.equal(extFromPath("/home/user/note", ".custom"), ".custom");
});

test("dirFromPath extracts directory for Linux and Windows", () => {
  assert.equal(dirFromPath("/home/user/notes/test.txt"), "/home/user/notes");
  assert.equal(dirFromPath("C:\\Users\\User\\notes\\test.txt"), "C:\\Users\\User\\notes");
  assert.equal(dirFromPath("test.txt"), ".");
});

test("joinPath joins directories and files without double separators", () => {
  assert.equal(joinPath("/home/user/notes", "test.txt"), "/home/user/notes/test.txt");
  assert.equal(joinPath("/home/user/notes/", "test.txt"), "/home/user/notes/test.txt");
  assert.equal(joinPath("C:\\Users\\notes", "test.txt"), "C:\\Users\\notes\\test.txt");
  assert.equal(joinPath("C:\\Users\\notes\\", "test.txt"), "C:\\Users\\notes\\test.txt");
  assert.equal(joinPath("", "test.txt"), "test.txt");
});

test("ensureDefaultExt appends .txt only when extension is missing", () => {
  assert.equal(ensureDefaultExt("/home/user/file"), "/home/user/file.txt");
  assert.equal(ensureDefaultExt("/home/user/file.txt"), "/home/user/file.txt");
  assert.equal(ensureDefaultExt("/home/user/file.markdown"), "/home/user/file.markdown");
});

test("defaultThreadBase generates a valid timestamped thread base name", () => {
  const base = defaultThreadBase();
  assert.match(base, /^note_\d{8}_\d{6}$/);
});

test("isDirty and displayTitle handle dirty state and title correctly", () => {
  const cleanTab = {
    title: "Note 1",
    path: null,
    content: "hello",
    saved: "hello",
  };
  assert.equal(isDirty(cleanTab), false);
  assert.equal(displayTitle(cleanTab), "Note 1");

  const dirtyTab = {
    title: "Note 1",
    path: null,
    content: "hello modified",
    saved: "hello",
  };
  assert.equal(isDirty(dirtyTab), true);
  // GNOME Text Editor style uses clean tab title and visual dot indicator
  assert.equal(displayTitle(dirtyTab), "Note 1");

  const pathTab = {
    title: "Untitled",
    path: "/home/user/notes/my_note.txt",
    content: "hello",
    saved: "hello",
  };
  assert.equal(displayTitle(pathTab), "my_note.txt");
});

test("createTab creates a tab with default properties", () => {
  const tab = createTab({ title: "Custom", content: "test" });
  assert.equal(tab.title, "Custom");
  assert.equal(tab.content, "test");
  assert.equal(tab.saved, "");
  assert.equal(tab.wrap, true);
  assert.equal(tab.snapN, 0);
  assert.equal(typeof tab.id, "number");
});

test("setFontSize clamps font size between FONT_MIN and FONT_MAX", () => {
  setFontSize(FONT_DEFAULT);
  assert.equal(getFontSize(), FONT_DEFAULT);

  setFontSize(FONT_MIN - 5);
  assert.equal(getFontSize(), FONT_MIN);

  setFontSize(FONT_MAX + 20);
  assert.equal(getFontSize(), FONT_MAX);

  setFontSize(16);
  assert.equal(getFontSize(), 16);
});

test("handleWheelZoom zooms in on Ctrl+Wheel Up and zooms out on Ctrl+Wheel Down", () => {
  setFontSize(16);

  let prevented = false;
  const zoomInEvent = {
    ctrlKey: true,
    metaKey: false,
    deltaY: -100,
    preventDefault: () => {
      prevented = true;
    },
  };
  handleWheelZoom(zoomInEvent);
  assert.equal(prevented, true);
  assert.equal(getFontSize(), 17);

  prevented = false;
  const zoomOutEvent = {
    ctrlKey: true,
    metaKey: false,
    deltaY: 100,
    preventDefault: () => {
      prevented = true;
    },
  };
  handleWheelZoom(zoomOutEvent);
  assert.equal(prevented, true);
  assert.equal(getFontSize(), 16);
});

test("handleWheelZoom ignores wheel events without Ctrl or Meta", () => {
  setFontSize(16);

  let prevented = false;
  const regularScrollEvent = {
    ctrlKey: false,
    metaKey: false,
    deltaY: -100,
    preventDefault: () => {
      prevented = true;
    },
  };
  handleWheelZoom(regularScrollEvent);
  assert.equal(prevented, false);
  assert.equal(getFontSize(), 16);
});

test("setTheme updates theme setting and resolveCurrentTheme returns correct value", () => {
  setTheme("dark");
  assert.equal(resolveCurrentTheme(), "dark");

  setTheme("light");
  assert.equal(resolveCurrentTheme(), "light");

  setTheme("system");
  // in node environment without matchMedia, fallback is 'light'
  assert.equal(resolveCurrentTheme(), "light");
});

test("setTheme rejects invalid theme values", () => {
  setTheme("dark");
  setTheme("invalid-theme");
  assert.equal(resolveCurrentTheme(), "dark");
});
