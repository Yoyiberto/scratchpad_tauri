# Plain Notepad

Minimal plain-text notepad built with Tauri 2 + vanilla JS.

## Features

- Multiple tabs
- Open / Save / Save As (default `.txt`, any extension allowed)
- Word wrap toggle (near-zero cost; CSS on the textarea)
- Theme: Light (default) / Dark / System
- Font zoom: Ctrl + mouse wheel (also Ctrl+= / Ctrl+- / Ctrl+0)
- Tab inserts indent in the editor (does not jump toolbar buttons)
- Session backup every 20 min (+ on blur/close) under `%LOCALAPPDATA%\plain-notepad\`
- Default Save path: `%LOCALAPPDATA%\plain-notepad\notes\`
- **Snap**: versioned chain `base_v1.ext`, `base_v2.ext`, … in the same folder

## Snapshots

Each tab has a **thread** name (`threadBase`):

- If the file is `ideas.txt` → thread `ideas` → snaps `ideas_v1.txt`, `ideas_v2.txt`, …
- If untitled → first Snap asks for a location and names like `note_YYYYMMDD_HHMMSS_v1.txt`
- Opening an existing `_vN` file links to the same thread stem

The editor is **not** cleared after a snap — rewrite freely, then Snap again for the next version.

## Dev

```bash
npm install
npm run tauri dev
```

## Shortcuts

| Key | Action |
|-----|--------|
| Ctrl+N | New tab |
| Ctrl+O | Open |
| Ctrl+S | Save |
| Ctrl+Shift+S | Save As |
| Ctrl+W | Close tab |
| Ctrl+Wheel | Zoom font |
| Ctrl+= / Ctrl+- | Zoom in / out |
| Ctrl+0 | Reset font size |
