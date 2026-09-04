[CmdletBinding()]
param()
$ErrorActionPreference = "Stop"
Push-Location (Join-Path $PSScriptRoot "..")
try {
    cargo fmt --check
    cargo check
    cargo test
    cargo clippy --all-targets --all-features -- -D warnings
    cargo build --release
    New-Item -ItemType Directory -Force dist | Out-Null
    Copy-Item target/release/fcz.exe dist/fcz-windows-x86_64.exe -Force
    $hash = (Get-FileHash dist/fcz-windows-x86_64.exe -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  fcz-windows-x86_64.exe" | Set-Content -Encoding ascii dist/SHA256SUMS
    & ./dist/fcz-windows-x86_64.exe --version
    Write-Host "Assets created under $((Resolve-Path dist).Path). Nothing was uploaded."
} finally { Pop-Location }
