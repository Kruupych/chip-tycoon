Param(
  [switch]$NoPrompt
)
$ErrorActionPreference = 'Stop'
Write-Host "[tauri] Checking Windows build prerequisites..." -ForegroundColor Cyan

# Rust toolchain (MSVC)
if (-not (Get-Command rustup -ErrorAction SilentlyContinue)) {
  Write-Warning "rustup not found. Install Rust from https://www.rust-lang.org/tools/install"
} else {
  Write-Host "Setting Rust default toolchain to stable-x86_64-pc-windows-msvc"
  rustup set default-host x86_64-pc-windows-msvc | Out-Null
  rustup default stable-x86_64-pc-windows-msvc | Out-Null
}

# Visual Studio Build Tools (detect via cl.exe or vswhere)
if (-not (Get-Command cl.exe -ErrorAction SilentlyContinue)) {
  $vswhere = "$Env:ProgramFiles(x86)\Microsoft Visual Studio\Installer\vswhere.exe"
  if (-not (Test-Path $vswhere)) {
    Write-Warning "Visual Studio Build Tools not detected. Install from https://visualstudio.microsoft.com/visual-cpp-build-tools/ or ensure cl.exe is in PATH."
  } else {
    Write-Host "Visual Studio installation detected via vswhere." -ForegroundColor Green
  }
} else {
  Write-Host "MSVC toolchain (cl.exe) detected in PATH." -ForegroundColor Green
}

# WebView2 Runtime
try {
  $wv2 = Get-ItemProperty -Path "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" -ErrorAction Stop
  Write-Host "WebView2 Runtime found: $($wv2.pv)"
} catch {
  Write-Warning "WebView2 Runtime not found. Install from https://developer.microsoft.com/microsoft-edge/webview2/"
}

# Node.js + pnpm
if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
  Write-Warning "Node.js not found. Install LTS from https://nodejs.org/en/download/package-manager"
}
if (-not (Get-Command pnpm -ErrorAction SilentlyContinue)) {
  Write-Host "Installing pnpm (corepack)"
  corepack enable 2>$null | Out-Null
  corepack prepare pnpm@latest --activate 2>$null | Out-Null
}

# Add @tauri-apps/cli as dev dependency in apps/mgmt-ui
$uiRoot = Join-Path (Get-Location) 'apps/mgmt-ui'
Push-Location $uiRoot
try {
  if (-not (Test-Path 'package.json')) {
    Write-Host "Creating minimal package.json in apps/mgmt-ui"
    @'
{
  "name": "mgmt-ui-root",
  "private": true,
  "devDependencies": {}
}
'@ | Set-Content -NoNewline -Path package.json -Encoding utf8
  }
  Write-Host "Ensuring @tauri-apps/cli is installed (dev dep)"
  pnpm add -D @tauri-apps/cli | Out-Null
} finally {
  Pop-Location
}
Write-Host "[tauri] Prerequisite check complete." -ForegroundColor Green
