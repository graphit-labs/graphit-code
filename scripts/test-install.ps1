#Requires -Version 5.1
param([Parameter(Mandatory = $true)][string]$Archive)
$ErrorActionPreference = 'Stop'
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) { throw 'This qualification requires native Windows.' }
$repo = Split-Path -Parent $PSScriptRoot
$Archive = (Resolve-Path -LiteralPath $Archive).Path
$root = Join-Path ([IO.Path]::GetTempPath()) ('graphit-installer-test-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root | Out-Null
$originalPath = $env:PATH
$originalUserPath = [Environment]::GetEnvironmentVariable('PATH', 'User')
# -Archive must not contact a remote service.
function Invoke-WebRequest { throw 'Local installation attempted a download.' }
function Invoke-RestMethod { throw 'Local installation attempted discovery.' }
function Assert-Package($Directory) {
    if ((Get-Content -Raw -LiteralPath (Join-Path $Directory 'graphit-runtime/broker/keep.txt')) -cne 'unrelated Broker data') { throw 'Broker data was changed.' }
    foreach ($item in @($manifest.executables) + @($manifest.libraries)) {
        if ((Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $Directory $item.file)).Hash -ine $item.sha256) { throw 'Installed runtime digest differs.' }
    }
    if (@(Get-ChildItem -LiteralPath $Directory -Force -Recurse | Where-Object { $_.Name -like '.graphit-install-*' -or $_.Name -like '*.replacing-*' }).Count) { throw 'Installer left staging or backup files.' }
}
function Assert-PublicationFailure($Failure) {
    if (-not $Failure -or -not $Failure.Exception.InnerException -or
        $Failure.Exception.Message -notlike "*Could not install 'graphit-model-server.exe'*" -or
        $Failure.Exception.Message -notmatch 'Files already installed: (?<published>.+)\. Close any process') { throw 'Missing publication error, cause or applied-file report.' }
    $published = $Matches.published
    if (-not $published.Contains('graphit-runtime/windows-libraries.json') -or -not $published.Contains('graphit.exe') -or $published.Contains('graphit-model-server.exe')) { throw 'Incorrect applied-file report.' }
    $cause = $Failure.Exception
    while ($cause.InnerException) { $cause = $cause.InnerException }
    if ($cause -isnot [IO.IOException] -and $cause -isnot [UnauthorizedAccessException]) { throw "Unexpected underlying publication error: $($cause.GetType().FullName)" }
}
try {
    $destination = Join-Path $root ('install ' + [char]0x03b1)
    New-Item -ItemType Directory -Path (Join-Path $destination 'graphit-runtime/broker') -Force | Out-Null
    Set-Content -LiteralPath (Join-Path $destination 'graphit-runtime/broker/keep.txt') -Value 'unrelated Broker data' -NoNewline
    & (Join-Path $repo 'install.ps1') -Archive $Archive -Dir $destination
    $manifest = Get-Content -Raw -LiteralPath (Join-Path $destination 'graphit-runtime/windows-libraries.json') | ConvertFrom-Json
    Assert-Package $destination
    $relocated = Join-Path $root ('relocated ' + [char]0x03b2)
    Move-Item -LiteralPath $destination -Destination $relocated
    try {
        $env:PATH = "$env:SystemRoot\System32;$env:SystemRoot"
        $version = & (Join-Path $relocated 'graphit.exe') --version
        if ($LASTEXITCODE -ne 0 -or -not $version) { throw 'Relocated package did not start.' }
    } finally { $env:PATH = $originalPath }
    & (Join-Path $repo 'install.ps1') -Archive $Archive -Dir $relocated
    Assert-Package $relocated
    Write-Host 'PASS: fresh installation, Unicode/spaces, relocation and replacement of existing files.'

    $blocked = Join-Path $relocated 'graphit-model-server.exe'
    $attributes = [IO.File]::GetAttributes($blocked)
    try {
        [IO.File]::SetAttributes($blocked, ($attributes -bor [IO.FileAttributes]::ReadOnly))
        $failure = $null
        try { & (Join-Path $repo 'install.ps1') -Archive $Archive -Dir $relocated } catch { $failure = $_ }
        Assert-PublicationFailure $failure
        if (-not ([IO.File]::GetAttributes($blocked) -band [IO.FileAttributes]::ReadOnly)) { throw 'Installer cleared the readonly attribute.' }
        Assert-Package $relocated
        Write-Host 'PASS: readonly destination rejects replacement, preserving bytes, readonly bit, applied-file report and unrelated files.'
    } finally { [IO.File]::SetAttributes($blocked, $attributes) }

    $handle = [IO.File]::Open($blocked, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::None)
    try {
        $firstByte = $handle.ReadByte()
        $failure = $null
        try { & (Join-Path $repo 'install.ps1') -Archive $Archive -Dir $relocated } catch { $failure = $_ }
        Assert-PublicationFailure $failure
        $handle.Position = 0
        if ($handle.ReadByte() -ne $firstByte) { throw 'Exclusive handle no longer reads its original content.' }
    } finally { $handle.Dispose() }
    Assert-Package $relocated
    Write-Host 'PASS: exclusive handle rejects replacement with original cause and explicit partial publication.'

    $linked = Join-Path $root ('linked prefix ' + [char]0x03b3)
    $linkCreated = $false
    try {
        New-Item -ItemType Junction -Path $linked -Target $relocated | Out-Null
        $linkCreated = $true
    } catch { Write-Host "SKIP: directory junction unavailable on this Windows host: $($_.Exception.Message)" }
    if ($linkCreated) {
        try {
            & (Join-Path $repo 'install.ps1') -Archive $Archive -Dir $linked
            if (-not ((Get-Item -LiteralPath $linked -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Installer replaced the configured directory junction.' }
            Assert-Package $relocated
            Write-Host 'PASS: configured directory junction remains linked and updates its target.'
        } finally { [IO.Directory]::Delete($linked) }
    }

    $damaged = Join-Path $root 'graphit-windows-amd64.tar.gz'
    Copy-Item -LiteralPath $Archive -Destination $damaged
    Set-Content -LiteralPath ($damaged + '.sha256') -Value (('0' * 64) + '  graphit-windows-amd64.tar.gz')
    $rejected = $false
    try { & (Join-Path $repo 'install.ps1') -Archive $damaged -Dir $relocated }
    catch { if ($_ -notmatch 'checksum mismatch') { throw }; $rejected = $true }
    if (-not $rejected) { throw 'Invalid package was accepted.' }
    Assert-Package $relocated

    # Tiny content fixtures test JSON semantics only; they are never executed as PE files.
    $manifestFixture = Join-Path $root 'manifest cases'
    $fixturePackage = Join-Path $manifestFixture 'graphit-windows-amd64'
    New-Item -ItemType Directory -Path (Join-Path $fixturePackage 'graphit-runtime') -Force | Out-Null
    $fixtureFiles = @{
        'graphit.exe' = 'contract-only graphit content'
        'graphit-model-server.exe' = 'contract-only model-server content'
        'graphit-runtime/LICENSE' = 'contract-only license'
        'graphit-runtime/build.json' = '{"fixture":"installer-manifest-contract"}'
    }
    foreach ($name in $fixtureFiles.Keys) { [IO.File]::WriteAllText((Join-Path $fixturePackage $name), $fixtureFiles[$name]) }
    $fixtureDeclarations = foreach ($name in @('graphit.exe', 'graphit-model-server.exe')) {
        @{ file = $name; sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $fixturePackage $name)).Hash }
    }
    $originalManifest = @{ platform = 'windows-amd64'; executables = @($fixtureDeclarations); libraries = @() } | ConvertTo-Json -Depth 5
    $fixtureManifest = Join-Path $manifestFixture 'graphit-windows-amd64/graphit-runtime/windows-libraries.json'
    $contractArchive = Join-Path $manifestFixture 'graphit-windows-amd64.tar.gz'
    $contractDestination = Join-Path $root 'manifest contract destination'
    New-Item -ItemType Directory -Path $contractDestination | Out-Null
    [IO.File]::WriteAllText((Join-Path $contractDestination 'keep.txt'), 'unrelated contract data')
    foreach ($case in @('reversed-executables', 'null-libraries', 'duplicate-executables')) {
        $fixture = $originalManifest | ConvertFrom-Json
        switch ($case) {
            'reversed-executables' { [array]::Reverse($fixture.executables) }
            'null-libraries' { $fixture.libraries = $null }
            'duplicate-executables' { $fixture.executables = @($fixture.executables[0], $fixture.executables[0]) }
        }
        $fixture | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $fixtureManifest -Encoding UTF8
        & tar -czf $contractArchive -C $manifestFixture graphit-windows-amd64
        if ($LASTEXITCODE -ne 0) { throw 'Could not package manifest fixture.' }
        Set-Content -LiteralPath ($contractArchive + '.sha256') -Value ((Get-FileHash -Algorithm SHA256 -LiteralPath $contractArchive).Hash + '  graphit-windows-amd64.tar.gz')
        $installedManifest = Join-Path $contractDestination 'graphit-runtime/windows-libraries.json'
        $beforeManifest = if (Test-Path -LiteralPath $installedManifest) { (Get-FileHash -Algorithm SHA256 -LiteralPath $installedManifest).Hash }
        $rejected = $false
        try { & (Join-Path $repo 'install.ps1') -Archive $contractArchive -Dir $contractDestination }
        catch {
            if ($case -eq 'reversed-executables' -or $_.Exception.Message -notlike 'Invalid Windows runtime*') { throw }
            $rejected = $true
        }
        if ($case -ne 'reversed-executables' -and -not $rejected) { throw "Accepted invalid runtime manifest: $case" }
        if ($rejected -and (Get-FileHash -Algorithm SHA256 -LiteralPath $installedManifest).Hash -cne $beforeManifest) { throw 'Invalid manifest changed the installation.' }
        foreach ($name in $fixtureFiles.Keys) {
            if ([IO.File]::ReadAllText((Join-Path $contractDestination $name)) -cne $fixtureFiles[$name]) { throw "Wrong contract fixture content: $name" }
        }
        if ([IO.File]::ReadAllText((Join-Path $contractDestination 'keep.txt')) -cne 'unrelated contract data') { throw 'Changed unrelated contract data.' }
        if (@(Get-ChildItem -LiteralPath $contractDestination -Recurse -File).Count -ne ($fixtureFiles.Count + 2)) { throw 'Unexpected files in contract destination.' }
        Write-Host "PASS: runtime manifest contract $case (synthetic archive $((Get-Item -LiteralPath $contractArchive).Length) bytes; no PE execution)."
    }
    Write-Host "PASS: complete Windows installer qualification on PowerShell $($PSVersionTable.PSVersion)."
} finally {
    [Environment]::SetEnvironmentVariable('PATH', $originalUserPath, 'User')
    $env:PATH = $originalPath
    Remove-Item -LiteralPath $root -Recurse -Force
}
