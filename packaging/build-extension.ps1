# Builds dist\otto.mcpb: Otto as a Claude desktop extension (like the Affinity extension).
#
# Install it by double-clicking dist\otto.mcpb, or in Claude desktop:
# Settings > Extensions > Advanced settings > Install extension.
# After updating Otto, run this again and reinstall to give Claude the new tools.
#
#   powershell -ExecutionPolicy Bypass -File packaging\build-extension.ps1 [-NoBuild]

param([switch]$NoBuild)
$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
$source = Join-Path $PSScriptRoot 'claude-extension'
$exe = Join-Path $root 'target\release\otto.exe'

if (-not $NoBuild) {
    Push-Location $root
    try {
        cargo build --release -p otto-app
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    } finally { Pop-Location }
}
if (-not (Test-Path $exe)) { throw "$exe not found; build it first (omit -NoBuild)" }

# The bundle: manifest and icon at the top level, the server binary under server\.
$stage = Join-Path $root 'target\mcpb-stage'
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force (Join-Path $stage 'server') | Out-Null
Copy-Item (Join-Path $source 'manifest.json'), (Join-Path $source 'icon.png') $stage
Copy-Item $exe (Join-Path $stage 'server\otto.exe')

# An .mcpb is a zip archive.
$dist = Join-Path $root 'dist'
New-Item -ItemType Directory -Force $dist | Out-Null
$zip = Join-Path $dist 'otto.zip'
$bundle = Join-Path $dist 'otto.mcpb'
Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zip -Force
Move-Item -Force $zip $bundle
Remove-Item -Recurse -Force $stage

$mb = [math]::Round((Get-Item $bundle).Length / 1MB, 1)
Write-Host "Built $bundle ($mb MB). Double-click it to install in Claude desktop."
