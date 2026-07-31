# Guandan client one-line install (Windows)
# Usage: irm https://raw.githubusercontent.com/SihanTeng/guan-dan/main/install.ps1 | iex

$ErrorActionPreference = "Stop"
$Repo = "SihanTeng/guan-dan"
$BinName = "guandan.exe"

function Write-Info {
    param([string]$Message)
    Write-Host "[INFO] $Message" -ForegroundColor Green
}

function Write-Warn {
    param([string]$Message)
    Write-Host "[WARN] $Message" -ForegroundColor Yellow
}

function Write-Err {
    param([string]$Message)
    Write-Host "[ERROR] $Message" -ForegroundColor Red
    exit 1
}

function Get-LatestVersion {
    Write-Info "Fetching latest release..."
    try {
        $response = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
        $version = $response.tag_name
        Write-Info "Latest version: $version"
        return $version
    }
    catch {
        Write-Err "Could not determine latest version (no releases yet?): $_"
    }
}

function Get-ArchSuffix {
    # Map Windows arch to release artifact names (amd64 / arm64)
    $arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString().ToLower()
    switch ($arch) {
        "x64" { return "amd64" }
        "arm64" { return "arm64" }
        default {
            # Fallback for older PowerShell
            if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { return "arm64" }
            return "amd64"
        }
    }
}

function Download-Binary {
    param([string]$Version)

    $arch = Get-ArchSuffix
    $binaryName = "guandan-windows-$arch.exe"
    $downloadUrl = "https://github.com/$Repo/releases/download/$Version/$binaryName"
    $checksumUrl = "$downloadUrl.sha256"

    Write-Info "Downloading client ($binaryName)..."

    $tempDir = Join-Path $env:TEMP "guandan-install"
    New-Item -ItemType Directory -Force -Path $tempDir | Out-Null
    $outputPath = Join-Path $tempDir $binaryName

    try {
        Invoke-WebRequest -Uri $downloadUrl -OutFile $outputPath -UseBasicParsing
    }
    catch {
        Write-Err "Download failed: $downloadUrl`n$_"
    }

    try {
        $sumFile = Join-Path $tempDir "$binaryName.sha256"
        Invoke-WebRequest -Uri $checksumUrl -OutFile $sumFile -UseBasicParsing
        $expected = (Get-Content $sumFile -Raw).Split(" ")[0].Trim()
        $actual = (Get-FileHash -Path $outputPath -Algorithm SHA256).Hash.ToLower()
        if ($expected.ToLower() -ne $actual) {
            Write-Err "Checksum mismatch"
        }
        Write-Info "Checksum OK"
    }
    catch {
        Write-Warn "Checksum verification skipped: $_"
    }

    Write-Info "Download complete"
    return $outputPath
}

function Install-Binary {
    param([string]$BinaryPath)

    Write-Info "Installing client..."

    $installDir = Join-Path $env:USERPROFILE ".guandan"
    New-Item -ItemType Directory -Force -Path $installDir | Out-Null

    $targetPath = Join-Path $installDir $BinName
    Copy-Item -Path $BinaryPath -Destination $targetPath -Force
    Write-Info "Installed to: $targetPath"

    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($userPath -notlike "*$installDir*") {
        Write-Info "Adding to user PATH..."
        [Environment]::SetEnvironmentVariable("Path", "$userPath;$installDir", "User")
        $env:Path = "$env:Path;$installDir"
        Write-Info "PATH updated (restart terminal if 'guandan' is not found)"
    }

    Remove-Item -Path (Split-Path $BinaryPath) -Recurse -Force -ErrorAction SilentlyContinue
}

function Main {
    Write-Host ""
    Write-Host "🥚 掼蛋 Guandan — client install" -ForegroundColor Cyan
    Write-Host ""

    $version = Get-LatestVersion
    $binaryPath = Download-Binary -Version $version
    Install-Binary -BinaryPath $binaryPath

    Write-Host ""
    Write-Info "✅ Install complete!"
    Write-Host ""
    Write-Host "  Play:  " -NoNewline
    Write-Host "guandan" -ForegroundColor Yellow
    Write-Host "  Help:  guandan --help"
    Write-Host ""
    Write-Host "  Default server: ws://127.0.0.1:9100" -ForegroundColor Gray
    Write-Host "  Override:       guandan --server ws://host:9100" -ForegroundColor Gray
    Write-Host ""
    Write-Host "  Tip: Use Windows Terminal for best emoji/color support." -ForegroundColor Gray
    Write-Host ""
}

Main
