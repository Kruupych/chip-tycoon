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

Write-Host "Running tauri build via pnpm dlx (from apps/mgmt-ui)"
Push-Location $uiRoot
try {
  pnpm dlx @tauri-apps/cli@2.8.4 build --project-path src-tauri --config src-tauri/tauri.conf.json
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
