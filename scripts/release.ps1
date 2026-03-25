param(
    [ValidateSet("patch", "minor", "major")]
    [string]$Level = "patch"
)

$ErrorActionPreference = "Stop"

Write-Host "▶ Releasing ($Level)..." -ForegroundColor Cyan

Write-Host "▶ Checking for uncommitted changes..." -ForegroundColor Yellow
$status = git status --porcelain
if ($status) {
    Write-Error "Working tree is dirty. Commit or stash changes first."
    exit 1
}

Write-Host "▶ Preview: upcoming changelog" -ForegroundColor Yellow
git-cliff --unreleased --strip header
Write-Host ""

$tagBefore = git describe --tags --abbrev=0 2>$null

Write-Host "▶ Running cargo release $Level --execute" -ForegroundColor Yellow
cargo release $Level --execute

$tagAfter = git describe --tags --abbrev=0 2>$null

if ($tagAfter -eq $tagBefore) {
    Write-Host "✗ Release aborted." -ForegroundColor Red
    exit 1
}

Write-Host "✓ Released $tagAfter! GitHub Actions will build and publish." -ForegroundColor Green
