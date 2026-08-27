# axur installer for Windows
#   irm https://raw.githubusercontent.com/agenzalabs/ax/main/install.ps1 | iex
#
# The release workflow has always built a Windows binary, but nothing could
# install it: install.sh has no Windows branch. This is that path, with the
# same rule — the download is verified against the published SHA256SUMS before
# it is put anywhere on PATH.

$ErrorActionPreference = 'Stop'

$Repo  = 'agenzalabs/ax'
$Bin   = 'axur'
$Asset = 'axur-windows-x64.exe'

function Write-Step($m) { Write-Host "-> $m" -ForegroundColor Cyan }
function Write-Ok($m)   { Write-Host "OK $m" -ForegroundColor Green }
function Write-Dim($m)  { Write-Host "   $m" -ForegroundColor DarkGray }
function Die($m)        { Write-Host $m -ForegroundColor Red; exit 1 }

if ([Environment]::Is64BitOperatingSystem -eq $false) {
    Die 'axur ships a 64-bit Windows binary only. Build from source: cargo install axur'
}

Write-Step 'Resolving latest release'
try {
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" `
        -Headers @{ 'User-Agent' = 'axur-installer' }
} catch {
    Die "Could not reach the GitHub API: $($_.Exception.Message)"
}

$version = if ($env:AXUR_VERSION) { $env:AXUR_VERSION } else { $release.tag_name }
if (-not $version) { Die 'Could not determine the latest release. Set AXUR_VERSION to pin a tag.' }

Write-Step "Installing $Bin $version"

$base = "https://github.com/$Repo/releases/download/$version"
$tmp  = Join-Path $env:TEMP ("axur-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $tmp | Out-Null

try {
    $binaryPath = Join-Path $tmp $Asset
    $sumsPath   = Join-Path $tmp 'SHA256SUMS'

    try {
        Invoke-WebRequest -Uri "$base/$Asset" -OutFile $binaryPath -UseBasicParsing
    } catch {
        Die "Download failed: $base/$Asset"
    }

    try {
        Invoke-WebRequest -Uri "$base/SHA256SUMS" -OutFile $sumsPath -UseBasicParsing
    } catch {
        Die 'Could not fetch SHA256SUMS - refusing to install unverified.'
    }

    Write-Step 'Verifying checksum'
    $line = Get-Content $sumsPath | Where-Object { $_ -match [regex]::Escape($Asset) } | Select-Object -First 1
    if (-not $line) { Die "$Asset is not listed in SHA256SUMS - refusing to install." }

    $expected = ($line -split '\s+')[0].ToLowerInvariant()
    $actual   = (Get-FileHash -Path $binaryPath -Algorithm SHA256).Hash.ToLowerInvariant()

    if ($expected -ne $actual) {
        Write-Host 'Checksum mismatch - refusing to install.' -ForegroundColor Red
        Write-Host "  expected $expected" -ForegroundColor Red
        Write-Host "  actual   $actual" -ForegroundColor Red
        exit 1
    }
    Write-Ok 'Checksum verified'
    Write-Dim $actual

    # Per-user by default: no elevation, and nothing outside the user's profile.
    $dest = Join-Path $env:LOCALAPPDATA 'axur\bin'
    New-Item -ItemType Directory -Path $dest -Force | Out-Null
    $target = Join-Path $dest "$Bin.exe"

    Copy-Item -Path $binaryPath -Destination $target -Force
    Write-Ok "Installed to $target"

    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if ($userPath -notlike "*$dest*") {
        [Environment]::SetEnvironmentVariable('Path', "$userPath;$dest", 'User')
        Write-Dim "Added $dest to your user PATH - open a new terminal to pick it up."
    }
} finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}

Write-Host ''
Write-Ok "axur $version installed"
Write-Host ''
Write-Host '  axur init                                    set up this project'
Write-Host '  axur install vercel-labs/skills#skills/find-skills'
Write-Host '  axur sync                                    install what the manifest declares'
Write-Host ''
Write-Dim 'Verify what it authorised to run:  axur audit'
