# Recuperacion de Plain Notepad

Este documento conserva el procedimiento para volver rapidamente a la version
estable que se usa diariamente.

## Version estable

- Release: `v0.1.0`
- Commit: `fa3146b698d3395d8bda5e7f1b5de9225bc63add`
- Release de GitHub: https://github.com/Yoyiberto/scratchpad_tauri/releases/tag/v0.1.0
- Instalador: `Plain.Notepad_0.1.0_x64-setup.exe`
- Ejecutable portable: `plain-notepad.exe`

El tag `v0.1.0` representa exactamente la version estable. No se debe mover,
borrar ni reutilizar ese tag para otra version.

## Rollback mas rapido

Si la version de desarrollo falla, cerrar Plain Notepad y reinstalar el
instalador del release `v0.1.0`. El instalador y el ejecutable portable estan
guardados en GitHub, por lo que no es necesario conservar una copia de 6 GB del
proyecto completo.

## Recuperar el codigo fuente

Desde un clon limpio del repositorio:

```bash
git fetch origin --tags
git switch --detach v0.1.0
npm install
npm run tauri build
```

Si se necesita una rama local basada en esa version:

```bash
git switch -c rollback-v0.1.0 v0.1.0
```

No usar `git reset --hard` como procedimiento normal de rollback. El tag y el
release permiten recuperar la version sin destruir cambios de desarrollo.

## Datos de usuario

El rollback del codigo no recupera ni revierte las notas del usuario. Antes de
probar cambios importantes, respaldar esta carpeta:

```powershell
$backup = "$env:USERPROFILE\Desktop\plain-notepad-backup-$(Get-Date -Format yyyyMMdd-HHmmss).zip"
Compress-Archive `
  -Path "$env:LOCALAPPDATA\plain-notepad" `
  -DestinationPath $backup
```

La carpeta contiene las notas y los respaldos de sesion de Plain Notepad.
Conservar estos respaldos separados del codigo.

## Desarrollo futuro

- `production/v0.1.0`: referencia de produccion; no modificar.
- `main`: integracion estable futura.
- `development`: nuevas features y pruebas que pueden romperse.
- `feature/*`: cambios individuales, cuando resulte conveniente.

Los directorios `node_modules/` y `src-tauri/target/` son generados. No deben
subirse al repositorio: se regeneran con `npm install` y `npm run tauri build`.
