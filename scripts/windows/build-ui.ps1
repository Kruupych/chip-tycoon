$ErrorActionPreference = 'Stop'
Write-Host "[tauri] Building Windows UI (Tauri)" -ForegroundColor Cyan

$root = (Get-Location)
$uiRoot = Join-Path $root 'apps/mgmt-ui'
$webRoot = Join-Path $uiRoot 'web'

if (-not (Test-Path (Join-Path $uiRoot 'src-tauri'))) {
  Write-Error "apps/mgmt-ui/src-tauri not found"
}

# Ensure pnpm
if (-not (Get-Command pnpm -ErrorAction SilentlyContinue)) {
  corepack enable
  corepack prepare pnpm@latest --activate
}

if (Test-Path (Join-Path $webRoot 'package.json')) {
  Write-Host "Installing web deps"
  pnpm --dir $webRoot i
} else {
  Write-Error "apps/mgmt-ui/web/package.json not found"
}

# Ensure tauri cli
Write-Host "Ensuring @tauri-apps/cli is present in web"
pnpm --dir $webRoot add -D @tauri-apps/cli | Out-Null

# Build Tauri
Write-Host "Building web frontend"
pnpm --dir $webRoot build

# Ensure Windows icon exists for tauri-build (icon.ico)
$iconsDir = Join-Path $uiRoot 'src-tauri/icons'
$icoPath = Join-Path $iconsDir 'icon.ico'
if (-not (Test-Path $icoPath)) {
  Write-Host "Generating default icon set for Tauri (missing icon.ico)"
  New-Item -ItemType Directory -Force -Path $iconsDir | Out-Null
  # Create a simple 256x256 PNG placeholder
  $pngPath = Join-Path $iconsDir 'base.png'
  Add-Type -AssemblyName System.Drawing
  $bmp = New-Object System.Drawing.Bitmap 256,256
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.Clear([System.Drawing.Color]::FromArgb(255,240,240,240))
  $g.Dispose()
  $bmp.Save($pngPath, [System.Drawing.Imaging.ImageFormat]::Png)
  $bmp.Dispose()
  # Generate icons via tauri CLI
  pnpm dlx @tauri-apps/cli@2.8.4 icon $pngPath | Out-Null
}

Write-Host "Running tauri build via pnpm dlx (from apps/mgmt-ui/src-tauri)"
Push-Location (Join-Path $uiRoot 'src-tauri')
try {
  pnpm dlx @tauri-apps/cli@2.8.4 build
} finally {
  Pop-Location
}

# Compute version from root Cargo.toml
$cargoToml = Get-Content (Join-Path $root 'Cargo.toml') -Raw
$ver = ($cargoToml -split "`n" | Where-Object { $_ -match '^version\s*=\s*"([^"]+)"' } | Select-Object -First 1)
if ($ver -match '"([^"]+)"') { $version = $Matches[1] } else { $version = '0.0.0' }
$dest = Join-Path $root ("dist/v$version/windows-x64/mgmt-ui")
New-Item -ItemType Directory -Force -Path $dest | Out-Null

$bundle1 = Join-Path $uiRoot 'src-tauri/target/release/bundle'
if (Test-Path $bundle1) {
  Write-Host "Copying bundles from $bundle1"
  Copy-Item -Recurse -Force "$bundle1/*" $dest
} else {
  Write-Warning "No Tauri bundle output found at $bundle1"
}
Write-Host "[tauri] Build complete: $dest" -ForegroundColor Green
