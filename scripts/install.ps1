<#
.SYNOPSIS
Builds cash from this checkout and installs it as the shell used day to day.

.DESCRIPTION
Builds cash.exe and copies it to %USERPROFILE%\.cash-dev\cash.exe, the program the
Windows Terminal profile for the dev build starts. The installed copy lives outside target\, so
any build folder can be deleted at any time, and a rebuild never has to work around
the shell that is running.

A running cash.exe cannot be overwritten, but it can be renamed: the installed copy is
renamed to cash.exe.old-<time> and the new one copied in beside it. Open tabs keep the
build they started with; new tabs start the new one. Old copies are deleted once no
tab runs them (here, and by scripts\tidy.ps1).

The `release` profile (thin LTO) builds in a fraction of the time of `dist` (fat LTO,
one codegen unit), which is what the release workflow ships. -CargoProfile dist installs
exactly that.

.EXAMPLE
powershell -File scripts\install.ps1
#>
[CmdletBinding()]
param(
    # The cargo profile to build: release (the quick one) or dist (what is shipped).
    [ValidateSet('release', 'dist')]
    [string]$CargoProfile = 'release',
    # Where the shell is installed; the Windows Terminal profile starts cash.exe there.
    # Not under %LOCALAPPDATA%: Windows Terminal from the Store sees a redirected AppData
    # of its own, where a folder another program made does not appear, and a profile
    # pointing there fails with 0x80070002 (file not found).
    [string]$Destination = (Join-Path $env:USERPROFILE '.cash-dev'),
    # Warn before building when the drive has less free space than this.
    [int]$MinFreeGB = 30
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot

Push-Location $repo
try {
    $drive = New-Object IO.DriveInfo ([IO.Path]::GetPathRoot($repo))
    $freeGB = [math]::Floor($drive.AvailableFreeSpace / 1GB)
    if ($freeGB -lt $MinFreeGB) {
        Write-Warning "Drive $($drive.Name) has only $freeGB GB free; scripts\tidy.ps1 -All deletes every build folder not in use."
    }

    cargo build --profile $CargoProfile --bin cash
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed (exit code $LASTEXITCODE)" }

    # Where cargo put it, which CARGO_TARGET_DIR or build.target-dir may have moved.
    $metadata = cargo metadata --format-version 1 --no-deps | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw "cargo metadata failed (exit code $LASTEXITCODE)" }
    $built = Join-Path $metadata.target_directory "$CargoProfile\cash.exe"
} finally {
    Pop-Location
}

New-Item -ItemType Directory -Force -Path $Destination | Out-Null
$installed = Join-Path $Destination 'cash.exe'
$setAside = $null
if (Test-Path -LiteralPath $installed) {
    $setAside = "$installed.old-$(Get-Date -Format 'yyyyMMdd-HHmmss')"
    Move-Item -LiteralPath $installed -Destination $setAside
}
try {
    Copy-Item -LiteralPath $built -Destination $installed
} catch {
    # Never leave the terminal profile pointing at nothing.
    if ($setAside) { Move-Item -LiteralPath $setAside -Destination $installed }
    throw
}

foreach ($old in @(Get-ChildItem -LiteralPath $Destination -Filter 'cash.exe.old*' -File -Force)) {
    try { Remove-Item -LiteralPath $old.FullName -Force } catch { }
}
$left = @(Get-ChildItem -LiteralPath $Destination -Filter 'cash.exe.old*' -File -Force).Count

& $installed --version
Write-Output "Installed to $installed ($CargoProfile profile). New cash tabs start it."
if ($left -gt 0) {
    Write-Output "$left older copy(ies) still run in open tabs; they are deleted once those close."
}
