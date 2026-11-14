import fs from "node:fs";
import { execSync } from "node:child_process";

console.log("=== Deploying Scratchpad to C:\\Tools ===");

// 1. Build release
console.log("Building release binary...");
execSync("npx tauri build --no-bundle", { stdio: "inherit" });

// 2. Kill running scratchpad if any
console.log("Closing running scratchpad instances...");
try {
  execSync("taskkill /F /IM scratchpad.exe", { stdio: "ignore" });
} catch {}

// 3. Find built binary
const targetCandidate1 = "C:\\rust-target\\tauri\\release\\scratchpad.exe";
const targetCandidate2 = "src-tauri\\target\\release\\scratchpad.exe";
let source = fs.existsSync(targetCandidate1) ? targetCandidate1 : targetCandidate2;

if (!fs.existsSync(source)) {
  console.error("Built binary not found at " + source);
  process.exit(1);
}

// 4. Copy to C:\Tools\scratchpad.exe
const dest = "C:\\Tools\\scratchpad.exe";
fs.copyFileSync(source, dest);
console.log(`Successfully deployed ${source} -> ${dest}`);
const stats = fs.statSync(dest);
console.log(`Size: ${stats.size} bytes, Modified: ${stats.mtime.toLocaleString()}`);
