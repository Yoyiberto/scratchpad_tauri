# Instala la version egui en C:\Tools\scratchpad.exe.
# La version Tauri (con el Snap arreglado) queda como C:\Tools\scratchpad-tauri.exe.
# No mata nada: si Scratchpad esta abierto, cierralo primero (asi guarda su sesion).
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

if (Get-Process scratchpad -ErrorAction SilentlyContinue) {
    Write-Host "Scratchpad esta abierto: cierralo y vuelve a correr este script." -ForegroundColor Yellow
    exit 1
}

cargo build --release
if ($LASTEXITCODE -ne 0) { exit 1 }

$tauri = "C:\rust-target\tauri\release\scratchpad.exe"
if (Test-Path $tauri) {
    Copy-Item $tauri "C:\Tools\scratchpad-tauri.exe" -Force
    Write-Host "Respaldo Tauri -> C:\Tools\scratchpad-tauri.exe"
}
Copy-Item "target\release\scratchpad.exe" "C:\Tools\scratchpad.exe" -Force
Write-Host "Instalado egui -> C:\Tools\scratchpad.exe" -ForegroundColor Green
