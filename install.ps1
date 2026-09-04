$ErrorActionPreference = "Stop"

$Repo = "rahulhalder123-456/fcz"
$AssetName = "fcz-windows-x86_64.exe"

Write-Host "Installing fcz from GitHub Releases..." -ForegroundColor Cyan

# Fetch latest release URL
$ApiUrl = "https://api.github.com/repos/$Repo/releases/latest"
try {
    $ReleaseData = Invoke-RestMethod -Uri $ApiUrl
    $AssetUrl = ($ReleaseData.assets | Where-Object { $_.name -eq $AssetName }).browser_download_url

    if (-not $AssetUrl) {
        Write-Host "Error: Could not find Windows asset $AssetName in latest release." -ForegroundColor Red
        exit 1
    }
} catch {
    Write-Host "Error fetching release data: $_" -ForegroundColor Red
    exit 1
}

# Define install location (~/.cargo/bin)
$InstallDir = Join-Path $env:USERPROFILE ".cargo\bin"
if (-not (Test-Path -Path $InstallDir)) {
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
}

$ExePath = Join-Path $InstallDir "fcz.exe"

Write-Host "Downloading $AssetUrl to $ExePath..."
Invoke-WebRequest -Uri $AssetUrl -OutFile $ExePath

Write-Host "fcz has been successfully installed to $ExePath!" -ForegroundColor Green
Write-Host "Make sure $InstallDir is in your PATH." -ForegroundColor Yellow
