# Scratchpad

> **Versión nueva (egui, `native/`)**: mismo bloc de notas en Rust puro, una sola barra delgada,
> ~4 MB y sin WebView. Lee la misma `session.json`, así que se puede volver a la versión Tauri.
> Instalar: cierra Scratchpad y corre `native\deploy.ps1` (deja `C:\Tools\scratchpad-tauri.exe` de respaldo).

## Historial automático escrito → limpio (versión egui)

No hace falta pulsar nada: la app detecta cuándo pasas de **escribir** a **limpiar** solo midiendo
cuántos caracteres entran y salen en cada edición (sin analizar el texto).

| Versión   | Cuándo se guarda |
|-----------|------------------|
| `escrito` | justo antes de empezar a limpiar (borrar una selección ≥ 40 caracteres, o borrar más de lo que escribes) |
| `limpio`  | al terminar: 2 min sin tocar la nota, cambiar de pestaña, cerrar, o volver a escribir mucho |
| `snap`    | manual: botón **Snap** o `Ctrl+Alt+S` (si la nota es un archivo, también lo guarda) |
| `final`   | al cerrar la pestaña o la app con cambios |

Nunca se guarda una versión idéntica a la anterior ni textos de < 3 caracteres.

Todo queda en texto plano en `%LOCALAPPDATA%\scratchpad\history\`, nunca en tus carpetas:

- `<nota>61113-095135_escrito.txt` — notas activas, un `.txt` por versión.
- `packs\AAAA-MM.jsonl` — "git gc": una vez al día, las notas sin cambios hace 7 días (y no abiertas)
  se empaquetan por mes en un solo archivo y se borra su carpeta. Es tu dataset mensual.
- `pares.jsonl` — menú ≡ → *Exportar dataset* (o `scratchpad.exe --dataset [salida]`):
  un JSON por línea con `antes`/`despues`, tipos, fechas y nº de caracteres, listo para analizar.

`Ctrl+H` abre el historial de la nota: clic en una versión para verla, **Restaurar** (lo actual queda como snap) o **Copiar**.

Atajos egui: `Ctrl+N/O/S/W`, `Ctrl+Shift+S` guardar como, `Ctrl+Alt+S` snap, `Ctrl+H` historial,
`Ctrl+Tab` pestaña siguiente, `Ctrl+E` corregir palabra (guarda en `corrections.json`), `Alt+Z` ajuste
de línea, `Ctrl+rueda` / `Ctrl+=` / `Ctrl+-` / `Ctrl+0` fuente, clic medio cierra pestaña.

---

## Versión Tauri (original)

Bloc de notas minimalista y rápido construido con Tauri 2 + JavaScript vanilla, interfaz estilo GNOME Text Editor / Ubuntu y compatibilidad completa con Windows.

## Características

- **Pestañas múltiples** con guardado de sesión automático.
- **Abrir / Guardar / Guardar como** (por defecto `.txt`, cualquier extensión permitida).
- **Ajuste de línea (Word Wrap)** configurable desde el menú (`Alt+Z`).
- **Selector de temas con iconos y soporte nativo de sistema**:
  - ☀️ **Light**: Modo normal sol (claro).
  - 🌙 **Dark**: Modo noche (oscuro).
  - 💻 **System**: Modo automático (sincronizado con el tema del sistema operativo Windows/Linux en tiempo real).
- **Zoom de fuente con rueda**: `Ctrl + Rueda del ratón` (o `Ctrl+=` / `Ctrl+-` / `Ctrl+0` para reiniciar).
- **Arrastre de ventana libre**: Arrastra la ventana manteniendo pulsada la barra superior (headerbar o espacio de pestañas) para moverla entre pantallas y acoplarla con Windows Snap.
- **Doble clic en cabecera**: Maximiza o restaura la ventana.
- **Snapshots versionados**: Cadena `base_v1.ext`, `base_v2.ext`, … en la misma carpeta. Snap no pide diálogo (notas nuevas van a `notes/`), guarda también el archivo, no duplica estados idénticos y nunca sobrescribe un `_vN` abierto.
- **Registro de correcciones de transcripción**: Doble clic o `Ctrl + Doble clic` en palabras para registrar correcciones y exportar a CSV/JSON.
- **Migración automática** de sesiones previas de `%LOCALAPPDATA%\plain-notepad`.

---

## Ubicación del Ejecutable y Lanzador de Windows

El ejecutable utilizado por el buscador y lanzador de Windows se encuentra instalado en:
```text
C:\Tools\scratchpad.exe
```

### Comando para compilar y desplegar en `C:\Tools`

Para compilar la versión optimizada, cerrar la instancia previa y actualizar automáticamente el ejecutable en `C:\Tools\`:
```powershell
npm run deploy:tools
```

O de forma manual:
```powershell
npm run build -- --no-bundle
Stop-Process -Name "scratchpad" -Force -ErrorAction SilentlyContinue
Copy-Item "C:\rust-target\tauri\release\scratchpad.exe" "C:\Tools\scratchpad.exe" -Force
```

---

## Desarrollo y Tests

```bash
# Instalar dependencias
npm install

# Modo desarrollo
npm run dev

# Ejecutar suite de pruebas (Node + Rust)
npm test

# Compilación local
npm run build -- --no-bundle
```

---

## Atajos de Teclado

| Atajo | Acción |
|-------|--------|
| `Ctrl+N` | Nueva pestaña |
| `Ctrl+O` | Abrir archivo |
| `Ctrl+S` | Guardar |
| `Ctrl+Shift+S` | Guardar como... |
| `Ctrl+W` | Cerrar pestaña |
| `Ctrl + Rueda` | Zoom del texto (aumentar/disminuir) |
| `Ctrl+=` / `Ctrl+-` | Aumentar / reducir tamaño de fuente |
| `Ctrl+0` | Restablecer tamaño de fuente (14px) |
| `Alt+Z` | Alternar ajuste de línea (Word Wrap) |
| `Ctrl + Doble Clic` | Abrir modal de corrección de transcripción en palabra |
