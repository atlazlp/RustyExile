#Requires -Version 5.1
param(
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"

$Root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
Set-Location $Root

$isccCandidates = @(
    (Join-Path $env:LOCALAPPDATA "Programs\Inno Setup 6\ISCC.exe"),
    "C:\Program Files (x86)\Inno Setup 6\ISCC.exe",
    "C:\Program Files\Inno Setup 6\ISCC.exe"
)
$iscc = $isccCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $iscc) {
    throw "Inno Setup 6 (ISCC.exe) was not found. Install it with: winget install -e --id JRSoftware.InnoSetup"
}

$version = (
    Select-String -Path (Join-Path $Root "Cargo.toml") -Pattern '^version\s*=\s*"([^"]+)"' |
    Select-Object -First 1
).Matches.Groups[1].Value
if (-not $version) {
    throw "Could not read version from Cargo.toml"
}

$cargo = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
if (-not (Test-Path $cargo)) {
    if (Get-Command cargo -ErrorAction SilentlyContinue) {
        $cargo = (Get-Command cargo).Source
    } else {
        throw "cargo was not found. Install Rust from https://rustup.rs and run this script again."
    }
}

if (-not $SkipBuild) {
    Write-Host "Building RustyExile $version (release)."
    & $cargo build --release -p RustyExile
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed with exit code $LASTEXITCODE"
    }
}

$metadata = & $cargo metadata --no-deps --format-version 1 --manifest-path (Join-Path $Root "Cargo.toml") | ConvertFrom-Json
$exe = Join-Path $metadata.target_directory "release\RustyExile.exe"
if (-not (Test-Path $exe)) {
    throw "Expected binary missing: $exe"
}

$dist = Join-Path $Root "dist"
$stage = Join-Path $dist "stage"
if (Test-Path $stage) {
    Remove-Item -Recurse -Force $stage
}
New-Item -ItemType Directory -Force -Path $stage | Out-Null
Copy-Item $exe (Join-Path $stage "RustyExile.exe")
Copy-Item -Recurse (Join-Path $Root "assets") (Join-Path $stage "assets")
Copy-Item (Join-Path $PSScriptRoot "portal.ico") (Join-Path $stage "portal.ico")

$iss = Join-Path $PSScriptRoot "RustyExile.iss"
Write-Host "Compiling Windows installer with Inno Setup."
& $iscc "/DMyAppVersion=$version" $iss
if ($LASTEXITCODE -ne 0) {
    throw "ISCC failed with exit code $LASTEXITCODE"
}

$setup = Join-Path $dist "RustyExile-windows-x86_64-setup.exe"
if (-not (Test-Path $setup)) {
    throw "Installer was not created: $setup"
}

Write-Host "Installer ready: $setup"
