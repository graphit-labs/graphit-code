[CmdletBinding()]
param(
    [string]$Dir,
    [string]$Version = $env:VERSION,
    [string]$Archive
)

$ErrorActionPreference = 'Stop'
if ($Archive -and $Version) { throw '-Archive and -Version are mutually exclusive' }
$Repo = 'graphit-labs/graphit-code'
$Platform = 'windows-amd64'
if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64' -and $env:PROCESSOR_ARCHITEW6432 -ne 'AMD64') {
    throw 'No release target for this operating system and architecture'
}
$InstallDir = if ($Dir) {
    $Dir
} elseif ($env:GRAPHIT_INSTALL_DIR) {
    $env:GRAPHIT_INSTALL_DIR
} else {
    Join-Path $env:LOCALAPPDATA 'Programs\graphit'
}
$InstallDir = [IO.Path]::GetFullPath($InstallDir)
$ArchiveName = "graphit-$Platform.tar.gz"
$BaseUrl = if ($Version) {
    "https://github.com/$Repo/releases/download/$Version"
} else {
    "https://github.com/$Repo/releases/latest/download"
}
$TempDir = Join-Path ([IO.Path]::GetTempPath()) "graphit-install-$(New-Guid)"
New-Item -ItemType Directory -Path $TempDir | Out-Null
try {
    $ArchiveFile = Join-Path $TempDir $ArchiveName
    $Checksum = Join-Path $TempDir 'checksum'
    if ($Archive) {
        Copy-Item -LiteralPath $Archive -Destination $ArchiveFile
        Copy-Item -LiteralPath "$Archive.sha256" -Destination $Checksum
    } else {
        Invoke-WebRequest -Uri "$BaseUrl/$ArchiveName" -OutFile $ArchiveFile -UseBasicParsing
        Invoke-WebRequest -Uri "$BaseUrl/$ArchiveName.sha256" -OutFile $Checksum -UseBasicParsing
    }
    $Pattern = '^([a-fA-F0-9]{64})\s{2}' + [regex]::Escape($ArchiveName) + '\r?\n?$'
    $Match = [regex]::Match((Get-Content -LiteralPath $Checksum -Raw), $Pattern)
    if (-not $Match.Success) { throw 'Missing or ambiguous release checksum' }
    $Expected = $Match.Groups[1].Value
    $Actual = (Get-FileHash -LiteralPath $ArchiveFile -Algorithm SHA256).Hash
    if ($Actual -ne $Expected) { throw 'Release archive checksum mismatch' }

    Get-Command tar -ErrorAction Stop | Out-Null
    & tar -xzf $ArchiveFile -C $TempDir
    if ($LASTEXITCODE -ne 0) { throw "Archive extraction failed with exit code $LASTEXITCODE" }
    $PackageRoot = Join-Path $TempDir "graphit-$Platform"
    $Required = @(
        'graphit.exe',
        'graphit-model-server.exe',
        'graphit-runtime\LICENSE',
        'graphit-runtime\build.json',
        'graphit-runtime\windows-libraries.json'
    )
    foreach ($Relative in $Required) {
        $Item = Get-Item -LiteralPath (Join-Path $PackageRoot $Relative) -ErrorAction Stop
        if ($Item.PSIsContainer -or $Item.Length -eq 0 -or ($Item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw "Release archive is missing a regular file: $Relative"
        }
    }
    $Runtime = Get-Content -LiteralPath (Join-Path $PackageRoot 'graphit-runtime\windows-libraries.json') -Raw | ConvertFrom-Json
    $Executables = @($Runtime.executables | ForEach-Object { $_.file } | Sort-Object)
    if ($Runtime.platform -ne $Platform -or $Executables.Count -ne 2 -or
        $Executables[0] -ne 'graphit-model-server.exe' -or $Executables[1] -ne 'graphit.exe' -or
        $null -eq $Runtime.libraries) { throw 'Invalid Windows runtime manifest' }
    $Seen = @{}
    foreach ($Library in $Runtime.libraries) {
        if ($Library.name -cnotmatch '^[A-Za-z0-9_.-]+\.dll$' -or $Library.file -cne $Library.name -or $Seen.ContainsKey($Library.name)) {
            throw 'Invalid Windows runtime DLL path'
        }
        $Seen[$Library.name] = $true
        $Item = Get-Item -LiteralPath (Join-Path $PackageRoot $Library.file) -ErrorAction Stop
        if ($Item.PSIsContainer -or $Item.Length -eq 0 -or ($Item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw "Release archive is missing a regular DLL: $($Library.file)"
        }
    }
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    Get-ChildItem -LiteralPath $PackageRoot -Force | Copy-Item -Destination $InstallDir -Recurse -Force

    $UserPath = [Environment]::GetEnvironmentVariable('PATH', 'User')
    $Entries = @($UserPath -split ';' | Where-Object { $_ })
    if ($Entries -notcontains $InstallDir) {
        [Environment]::SetEnvironmentVariable('PATH', (($Entries + $InstallDir) -join ';'), 'User')
        $env:PATH = "$env:PATH;$InstallDir"
    }
    Write-Host "Installed graphit and graphit-model-server in $InstallDir"
    Write-Host 'Open a new terminal to pick up PATH changes.'
} finally {
    Remove-Item -LiteralPath $TempDir -Recurse -Force -ErrorAction SilentlyContinue
}
