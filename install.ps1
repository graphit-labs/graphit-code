#Requires -Version 5.1
[CmdletBinding()]
param(
    [string]$Dir,
    [string]$Version = $env:VERSION,
    [string]$Archive,
    [switch]$Help
)

if ($Help) {
    @'
Install Graphit executables and runtime assets.

Usage: ./install.ps1 [OPTIONS]

Options:
  -Dir PATH       Installation directory (default: LocalAppData/Programs/graphit)
  -Version TAG    Release tag (default: latest)
  -Archive PATH   Verified local archive; requires PATH.sha256, no download
  -Help           Show this help

Choose either -Version or -Archive.
'@
    exit 0
}

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
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
        Write-Host 'Downloading Graphit release...'
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
    $RootName = "graphit-$Platform"
    $Members = @(& tar -tzf $ArchiveFile)
    if ($LASTEXITCODE -ne 0) { throw "Archive listing failed with exit code $LASTEXITCODE" }
    $MemberNames = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    $PackageFiles = [Collections.Generic.List[string]]::new()
    foreach ($Member in $Members) {
        $Name = $Member.TrimEnd('/')
        if (-not $MemberNames.Add($Name)) { throw 'Duplicate release archive member' }
        if ($Name -ceq $RootName) { continue }
        if (-not $Name.StartsWith($RootName + '/', [StringComparison]::Ordinal)) { throw 'Release archive member is outside the package root' }
        $Relative = $Name.Substring($RootName.Length + 1)
        foreach ($Part in ($Relative -split '/')) {
            if (-not $Part -or $Part -eq '.' -or $Part -eq '..' -or $Part -match '[\\:\x00-\x1f]' -or $Part -match '[. ]$') {
                throw 'Invalid release archive member path'
            }
        }
        if (-not $Member.EndsWith('/')) { $PackageFiles.Add($Relative) }
    }
    $MemberTypes = @(& tar -tvzf $ArchiveFile)
    if ($LASTEXITCODE -ne 0) { throw "Archive type listing failed with exit code $LASTEXITCODE" }
    foreach ($MemberType in $MemberTypes) {
        if (-not $MemberType.StartsWith('-') -and -not $MemberType.StartsWith('d')) { throw 'Release archive members must be regular files or directories' }
    }
    & tar -xzf $ArchiveFile -C $TempDir
    if ($LASTEXITCODE -ne 0) { throw "Archive extraction failed with exit code $LASTEXITCODE" }
    $PackageRoot = Join-Path $TempDir "graphit-$Platform"
    foreach ($Relative in $PackageFiles) {
        $Item = Get-Item -LiteralPath (Join-Path $PackageRoot $Relative) -ErrorAction Stop
        if ($Item -isnot [IO.FileInfo] -or ($Item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw "Release archive member must be a regular file: $Relative"
        }
    }
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
    if ($Runtime.platform -cne $Platform -or $Runtime.executables -isnot [array] -or $Runtime.libraries -isnot [array]) {
        throw 'Invalid Windows runtime manifest: platform or array fields'
    }
    # Filenames form an exact set; linguistic sorting differs between .NET Framework and .NET.
    $Executables = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    foreach ($Executable in $Runtime.executables) {
        if ($Executable.file -isnot [string] -or -not $Executables.Add($Executable.file)) {
            throw 'Invalid Windows runtime executable declaration'
        }
    }
    if ($Executables.Count -ne 2 -or -not $Executables.Contains('graphit-model-server.exe') -or -not $Executables.Contains('graphit.exe')) {
        throw 'Invalid Windows runtime executable set'
    }
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
    $Ordered = @($PackageFiles | Where-Object { $_ -cne 'graphit.exe' -and $_ -cne 'graphit-model-server.exe' }) + @('graphit.exe', 'graphit-model-server.exe')
    $InstalledFiles = [Collections.Generic.List[string]]::new()
    foreach ($Relative in $Ordered) {
        $Destination = Join-Path $InstallDir $Relative
        $Parent = Split-Path -Parent $Destination
        $Stage = Join-Path $Parent ('.graphit-install-' + [guid]::NewGuid().ToString('N'))
        try {
            New-Item -ItemType Directory -Path $Parent -Force | Out-Null
            Copy-Item -LiteralPath (Join-Path $PackageRoot $Relative) -Destination $Stage
            if (Test-Path -LiteralPath $Destination) {
                # NullString supplies a null backup name; metadata/ACL errors stay fatal.
                [IO.File]::Replace($Stage, $Destination, [System.Management.Automation.Language.NullString]::Value, $false)
            } else {
                [IO.File]::Move($Stage, $Destination)
            }
            $InstalledFiles.Add($Relative)
        } catch {
            $Completed = if ($InstalledFiles.Count) { $InstalledFiles -join ', ' } else { '(none)' }
            throw [IO.IOException]::new("Could not install '$Relative'. Files already installed: $Completed. Close any process using the destination, resolve the reported error and rerun the installer. Earlier files remain installed. Cause: $($_.Exception.Message)", $_.Exception)
        } finally {
            if (Test-Path -LiteralPath $Stage) { Remove-Item -LiteralPath $Stage -Force }
        }
    }

    $UserPath = [Environment]::GetEnvironmentVariable('PATH', 'User')
    $Entries = @($UserPath -split ';' | Where-Object { $_ })
    if ($Entries -notcontains $InstallDir) {
        [Environment]::SetEnvironmentVariable('PATH', (($Entries + $InstallDir) -join ';'), 'User')
        $env:PATH = "$env:PATH;$InstallDir"
        [IO.File]::WriteAllText((Join-Path $InstallDir 'graphit-runtime/user-path-added'), $InstallDir)
    }
    Write-Host "Installed graphit and graphit-model-server in $InstallDir"
    Write-Host 'Open a new terminal to pick up PATH changes.'
} finally {
    Remove-Item -LiteralPath $TempDir -Recurse -Force
}

Write-Output 'Next: graphit --help'
