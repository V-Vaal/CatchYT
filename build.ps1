# CatchYT - local Windows build script.
# Usage:  powershell -ExecutionPolicy Bypass -File .\build.ps1
#         powershell -ExecutionPolicy Bypass -File .\build.ps1 -Run

param(
    [switch]$Run,          # launch the app after building
    [switch]$Debug         # build a debug binary instead of release
)

$ErrorActionPreference = "Stop"
$projectRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location -LiteralPath $projectRoot

function Have($name) { $null -ne (Get-Command $name -ErrorAction SilentlyContinue) }

Write-Host "== CatchYT build ==" -ForegroundColor Magenta

if (-not (Have "cargo")) {
    Write-Host "Rust (cargo) not found." -ForegroundColor Yellow
    Write-Host "Install it once from https://rustup.rs - pick the default (MSVC) toolchain, then reopen this terminal." -ForegroundColor Yellow
    exit 1
}

cargo --version
rustc --version

Write-Host "`n== Running tests ==" -ForegroundColor Magenta
cargo test --all

Write-Host "`n== Compiling ==" -ForegroundColor Magenta
if ($Debug) {
    cargo build
    $exe = "target\debug\catchyt.exe"
} else {
    cargo build --release
    $exe = "target\release\catchyt.exe"
}

if (Test-Path $exe) {
    $full = (Resolve-Path $exe).Path
    if (-not $Debug) {
        # Keep one obvious, user-facing executable at the repository root. The
        # temporary copy plus File.Replace avoids leaving a truncated artifact.
        $portable = Join-Path $projectRoot "catchyt.exe"
        $staging = "$portable.new"
        $backup = "$portable.old"
        try {
            Copy-Item -LiteralPath $full -Destination $staging -Force
            if (Test-Path -LiteralPath $portable) {
                [System.IO.File]::Replace($staging, $portable, $backup, $true)
                Remove-Item -LiteralPath $backup -Force -ErrorAction SilentlyContinue
            } else {
                Move-Item -LiteralPath $staging -Destination $portable
            }
        } catch {
            Remove-Item -LiteralPath $staging -Force -ErrorAction SilentlyContinue
            throw "Cannot publish catchyt.exe. Close any running CatchYT window, then rebuild. $($_.Exception.Message)"
        }
        $full = $portable
    }

    $hash = (Get-FileHash -LiteralPath $full -Algorithm SHA256).Hash
    Write-Host "`nBuilt: $full" -ForegroundColor Green
    Write-Host "SHA-256: $hash" -ForegroundColor DarkGray
    if ($Run) {
        Write-Host "Launching..." -ForegroundColor Magenta
        & $full
    }
} else {
    Write-Host "Build finished but $exe was not found." -ForegroundColor Red
    exit 1
}
