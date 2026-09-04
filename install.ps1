[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$Repo = "rahulhalder123-456/fc"
$AssetName = "fcz-windows-x86_64.exe"
$InstallDir = if ($env:FCZ_INSTALL_DIR) { $env:FCZ_INSTALL_DIR } else { Join-Path $HOME ".local\bin" }
$ExePath = Join-Path $InstallDir "fcz.exe"
$TempDir = Join-Path ([IO.Path]::GetTempPath()) ("fcz-install-" + [guid]::NewGuid())

try {
    $architecture = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
    if ($architecture -ne "X64") {
        throw "Unsupported architecture. This release requires Windows x86_64."
    }

    Write-Host "Installing fcz from $Repo..." -ForegroundColor Cyan
    $headers = @{ "User-Agent" = "fcz-installer"; "Accept" = "application/vnd.github+json" }
    try {
        $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" -Headers $headers
    } catch {
        throw "GitHub API request failed. Check your internet connection and that a release exists. $($_.Exception.Message)"
    }

    $asset = $release.assets | Where-Object name -eq $AssetName | Select-Object -First 1
    if (-not $asset) {
        throw "Release '$($release.tag_name)' does not contain $AssetName. Build from source with: cargo install --git https://github.com/$Repo.git"
    }

    New-Item -ItemType Directory -Force -Path $TempDir | Out-Null
    $download = Join-Path $TempDir $AssetName
    try {
        Invoke-WebRequest -Uri $asset.browser_download_url -Headers $headers -OutFile $download
    } catch {
        throw "Binary download failed. $($_.Exception.Message)"
    }
    if (-not (Test-Path -LiteralPath $download -PathType Leaf) -or (Get-Item -LiteralPath $download).Length -eq 0) {
        throw "Downloaded binary is missing or empty."
    }

    $checksums = $release.assets | Where-Object name -eq "SHA256SUMS" | Select-Object -First 1
    if ($checksums) {
        $checksumFile = Join-Path $TempDir "SHA256SUMS"
        Invoke-WebRequest -Uri $checksums.browser_download_url -Headers $headers -OutFile $checksumFile
        $line = Get-Content -LiteralPath $checksumFile | Where-Object { $_ -match "\s\*?$([regex]::Escape($AssetName))$" } | Select-Object -First 1
        if (-not $line) { throw "SHA256SUMS has no entry for $AssetName." }
        $expected = ($line -split "\s+")[0].ToLowerInvariant()
        $actual = (Get-FileHash -LiteralPath $download -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actual -ne $expected) { throw "SHA-256 verification failed for $AssetName." }
        Write-Host "SHA-256 verified." -ForegroundColor Green
    } else {
        Write-Warning "This release has no SHA256SUMS asset; checksum verification was skipped."
    }

    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    Move-Item -LiteralPath $download -Destination $ExePath -Force

    if ($env:FCZ_SKIP_PATH_UPDATE -ne "1") {
        $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
        $entries = @($userPath -split ";" | Where-Object { $_ })
        if (-not ($entries | Where-Object { $_.TrimEnd("\") -ieq $InstallDir.TrimEnd("\") })) {
            $newPath = (($entries + $InstallDir) -join ";")
            [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
            $env:Path = "$InstallDir;$env:Path"
            Write-Host "Added $InstallDir to your user PATH. Open a new terminal to use it." -ForegroundColor Yellow
        }
    }

    & $ExePath --version
    if ($LASTEXITCODE -ne 0) { throw "Installed binary validation failed with exit code $LASTEXITCODE." }
    Write-Host "Installed fcz to $ExePath" -ForegroundColor Green
} catch {
    Write-Error $_.Exception.Message
    exit 1
} finally {
    if (Test-Path -LiteralPath $TempDir) { Remove-Item -LiteralPath $TempDir -Recurse -Force }
}
