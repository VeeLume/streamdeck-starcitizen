$ErrorActionPreference = "Stop"

$PluginId   = "icu.veelume.starcitizen"
$SdPlugin   = "$PSScriptRoot/../icu.veelume.starcitizen.sdPlugin"
$BinDir     = "$SdPlugin/bin"
$PluginExe  = "$BinDir/$PluginId.exe"
$GenExe     = "$BinDir/toggle-groups-gen.exe"

# ── Stop plugin ──────────────────────────────────────────────────────────────────

Write-Host ">> Stopping plugin..." -ForegroundColor Yellow
streamdeck stop $PluginId 2>$null

# ── Build ────────────────────────────────────────────────────────────────────────

Write-Host ">> Building release..." -ForegroundColor Yellow
cargo build --release
if ($LASTEXITCODE -ne 0) {
    Write-Host "!! Build failed" -ForegroundColor Red
    exit 1
}

# ── Wait for process exit ────────────────────────────────────────────────────────

$ExeName = [System.IO.Path]::GetFileNameWithoutExtension($PluginExe)
$Timeout = 60
for ($i = 0; $i -lt $Timeout; $i++) {
    if (-not (Get-Process -Name $ExeName -ErrorAction SilentlyContinue)) { break }
    if ($i -eq 0) { Write-Host ">> Waiting for $ExeName to exit..." -ForegroundColor Yellow }
    Start-Sleep -Seconds 1
}

# ── Copy binaries ────────────────────────────────────────────────────────────────

Write-Host ">> Copying binaries..." -ForegroundColor Yellow
Copy-Item "target/release/plugin.exe" $PluginExe -Force
Copy-Item "target/release/toggle-groups-gen.exe" $GenExe -Force

# ── Restart plugin ───────────────────────────────────────────────────────────────

Write-Host ">> Restarting plugin..." -ForegroundColor Yellow
streamdeck restart $PluginId

Write-Host ">> Done" -ForegroundColor Green
