param(
    [Parameter(Mandatory=$true)]
    [string]$PluginName
)

if (-not $PluginName) { Write-Error 'Plugin name must not be empty'; exit 1 }

Write-Host "Setting up plugin: $PluginName" -ForegroundColor Cyan

Write-Host '▶ Replacing starcitizen in all files...' -ForegroundColor Yellow
Get-ChildItem -Recurse -File -Exclude '*.exe','*.ttf','*.png','*.js','*.cmd' | ForEach-Object {
    $c = Get-Content $_.FullName -Raw -ErrorAction SilentlyContinue
    if ($c -and $c.Contains('starcitizen')) {
        $c.Replace('starcitizen', $PluginName) | Set-Content $_.FullName -NoNewline
        Write-Host "  ✓ $($_.FullName)"
    }
}

Write-Host '▶ Renaming sdPlugin directory...' -ForegroundColor Yellow
$oldDir = Join-Path $PWD "icu.veelume.starcitizen.sdPlugin"
$newDir = Join-Path $PWD "icu.veelume.$PluginName.sdPlugin"
if (Test-Path $oldDir) {
    Rename-Item $oldDir $newDir
    Write-Host "  ✓ $newDir"
} else {
    Write-Host "  ℹ Already renamed or not found: $oldDir"
}

Write-Host '▶ Downloading sdpi-components.js...' -ForegroundColor Yellow
$piDir = Join-Path $newDir 'pi'
$sdpiDest = Join-Path $piDir 'sdpi-components.js'
if (-not (Test-Path $sdpiDest)) {
    Invoke-WebRequest -Uri 'https://sdpi-components.dev/releases/v4/sdpi-components.js' -OutFile $sdpiDest
    Write-Host "  ✓ $sdpiDest"
} else {
    Write-Host '  ℹ sdpi-components.js already exists'
}

Write-Host '▶ Enabling git hooks...' -ForegroundColor Yellow
git config core.hooksPath .githooks 2>$null

Write-Host '▶ Linking plugin for development...' -ForegroundColor Yellow
$bundle = "icu.veelume.$PluginName.sdPlugin"
if (Test-Path $bundle) {
    streamdeck link $bundle
} else {
    Write-Host "  ⚠ Bundle not found: $bundle" -ForegroundColor Red
}

Write-Host '▶ Cleaning up...' -ForegroundColor Yellow
$cmdFile = Join-Path $PWD 'copy-sdpi.cmd'
if (Test-Path $cmdFile) {
    Remove-Item $cmdFile -Force
    Write-Host '  ✓ Removed copy-sdpi.cmd'
}

Write-Host '▶ Setting up Claude Code integrations...' -ForegroundColor Yellow
if (Get-Command claude -ErrorAction SilentlyContinue) {
    claude mcp add --scope project context7 -- npx -y @upstash/context7-mcp
    Write-Host '  ✓ context7 MCP added (writes .mcp.json)'
    claude plugin install --scope project commit-commands
    Write-Host '  ✓ commit-commands plugin installed'
} else {
    Write-Host '  ⚠ claude CLI not found, skipping Claude Code setup' -ForegroundColor Red
}

Write-Host '▶ Initializing git repository...' -ForegroundColor Yellow
if (-not (Test-Path '.git')) {
    git init
    git config core.hooksPath .githooks
    git add -A
    git commit -m 'chore: initial project scaffold'
    Write-Host '  ✓ Repository initialized with initial commit'
} else {
    Write-Host '  ℹ Git repository already exists'
}

Write-Host '✓ Setup complete!' -ForegroundColor Green
