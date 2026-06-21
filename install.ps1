# Graphit Code Installer for Windows
# Usage: irm https://raw.githubusercontent.com/graphit-labs/graphit-code/main/install.ps1 | iex
#
# Or to install silently:
#   $env:GRAPHIT_INSTALL_DIR = "$env:USERPROFILE\.graphit\bin"
#   irm https://raw.githubusercontent.com/graphit-labs/graphit-code/main/install.ps1 | iex

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$Repo      = "graphit-labs/graphit-code"
$BinName   = "graphit"
$Platform  = "windows-amd64"

# ── Install directory ─────────────────────────────────────────────────────────
$InstallDir = if ($env:GRAPHIT_INSTALL_DIR) {
    $env:GRAPHIT_INSTALL_DIR
} else {
    Join-Path $env:USERPROFILE ".graphit\bin"
}

# ── Helpers ───────────────────────────────────────────────────────────────────
function Write-Step   { param($msg) Write-Host "  → $msg" -ForegroundColor Cyan }
function Write-Ok     { param($msg) Write-Host "  ✓ $msg" -ForegroundColor Green }
function Write-Warn   { param($msg) Write-Host "  ⚠ $msg" -ForegroundColor Yellow }
function Write-Fail   { param($msg) Write-Host "  ✗ $msg" -ForegroundColor Red; exit 1 }

# ── Banner ────────────────────────────────────────────────────────────────────
Write-Host ""
Write-Host "Graphit Code Installer" -ForegroundColor White -BackgroundColor DarkBlue
Write-Host "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━" -ForegroundColor DarkBlue
Write-Host ""

# ── Fetch latest version ──────────────────────────────────────────────────────
Write-Step "Fetching latest version..."
try {
    $ApiUrl  = "https://api.github.com/repos/$Repo/releases/latest"
    $Headers = @{ "User-Agent" = "graphit-installer/1"; "Accept" = "application/vnd.github+json" }
    $Release = Invoke-RestMethod -Uri $ApiUrl -Headers $Headers
    $Version = $Release.tag_name
} catch {
    Write-Fail "Could not fetch latest version: $_"
}
Write-Ok "Latest version: $Version"

# ── URLs ──────────────────────────────────────────────────────────────────────
$ArchiveName  = "$BinName-$Platform.tar.gz"
$BaseUrl      = "https://github.com/$Repo/releases/download/$Version"
$ArchiveUrl   = "$BaseUrl/$ArchiveName"
$ChecksumUrl  = "$BaseUrl/checksums.sha256"

# ── Temp dir ──────────────────────────────────────────────────────────────────
$TmpDir = Join-Path $env:TEMP "graphit-install-$(New-Guid)"
New-Item -ItemType Directory -Path $TmpDir | Out-Null
$TmpArchive  = Join-Path $TmpDir $ArchiveName
$TmpChecksum = Join-Path $TmpDir "checksums.sha256"

try {
    # ── Download ──────────────────────────────────────────────────────────────
    Write-Step "Downloading $ArchiveName..."
    Invoke-WebRequest -Uri $ArchiveUrl -OutFile $TmpArchive -UseBasicParsing
    Write-Ok "Downloaded archive"

    Write-Step "Downloading checksums..."
    Invoke-WebRequest -Uri $ChecksumUrl -OutFile $TmpChecksum -UseBasicParsing

    # ── Verify checksum ───────────────────────────────────────────────────────
    Write-Step "Verifying checksum..."
    $ChecksumContent = Get-Content $TmpChecksum
    $Expected = ($ChecksumContent | Where-Object { $_ -match [regex]::Escape($ArchiveName) } |
                 ForEach-Object { ($_ -split '\s+')[0] } | Select-Object -First 1)
    if (-not $Expected) {
        Write-Fail "Checksum for $ArchiveName not found in checksums.sha256"
    }
    $Actual = (Get-FileHash -Path $TmpArchive -Algorithm SHA256).Hash.ToLower()
    if ($Actual -ne $Expected.ToLower()) {
        Write-Fail "Checksum mismatch!`n  Expected: $Expected`n  Got:      $Actual"
    }
    Write-Ok "Checksum verified"

    # ── Extract ───────────────────────────────────────────────────────────────
    Write-Step "Extracting archive..."
    # `tar` is available in Windows 10+ (build 17063+)
    if (Get-Command tar -ErrorAction SilentlyContinue) {
        & tar -xzf $TmpArchive -C $TmpDir
    } else {
        Write-Fail "'tar' not found. Please install Windows 10 1803+ or Git for Windows."
    }

    # Find binary (graphit-windows-amd64.exe or graphit.exe)
    $TmpBin = $null
    foreach ($candidate in @(
        Join-Path $TmpDir "$BinName-$Platform.exe"
        Join-Path $TmpDir "$BinName.exe"
    )) {
        if (Test-Path $candidate) { $TmpBin = $candidate; break }
    }
    if (-not $TmpBin) {
        Write-Fail "Could not find binary in extracted archive"
    }
    Write-Ok "Archive extracted"

    # ── Install ───────────────────────────────────────────────────────────────
    Write-Step "Installing to $InstallDir..."
    if (-not (Test-Path $InstallDir)) {
        New-Item -ItemType Directory -Path $InstallDir | Out-Null
    }
    $Dest = Join-Path $InstallDir "$BinName.exe"
    Copy-Item -Path $TmpBin -Destination $Dest -Force
    Write-Ok "Installed to $Dest"

    # ── Add to PATH ───────────────────────────────────────────────────────────
    $UserPath = [System.Environment]::GetEnvironmentVariable("PATH", "User")
    if ($UserPath -notlike "*$InstallDir*") {
        Write-Step "Adding $InstallDir to user PATH..."
        [System.Environment]::SetEnvironmentVariable(
            "PATH",
            "$UserPath;$InstallDir",
            "User"
        )
        # Also update current session
        $env:PATH = "$env:PATH;$InstallDir"
        Write-Ok "PATH updated (restart your terminal to take full effect)"
    } else {
        Write-Ok "$InstallDir already in PATH"
    }

    # ── Done ─────────────────────────────────────────────────────────────────
    Write-Host ""
    Write-Host "  Installation complete!" -ForegroundColor Green
    Write-Host ""
    Write-Host "  Next steps:" -ForegroundColor White
    Write-Host ""
    Write-Host "    1. Open a new terminal (to pick up PATH)" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "    2. Run initial setup:" -ForegroundColor Cyan
    Write-Host "         graphit setup" -ForegroundColor White
    Write-Host ""
    Write-Host "    3. Initialize your project:" -ForegroundColor Cyan
    Write-Host "         graphit init --ide <antigravity|gemini|claude|cursor|kiro|codex|opencode>" -ForegroundColor White
    Write-Host ""
    Write-Host "    4. Docs: https://github.com/$Repo/tree/main/docs" -ForegroundColor Cyan
    Write-Host ""

} finally {
    Remove-Item -Recurse -Force -Path $TmpDir -ErrorAction SilentlyContinue
}
