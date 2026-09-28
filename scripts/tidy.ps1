<#
.SYNOPSIS
Deletes the build output nothing needs any more, and warns when the drive runs low.

.DESCRIPTION
Every checkout of cash, the main one and each worktree, has a target\ folder of its own,
and each can grow to tens of gigabytes. Cargo never deletes them (rust-lang/cargo#13136).
This script does, for this repository only. It deletes:

- the folders under each checkout's target\ (debug, release, dist, and any made with
  --target-dir) that have not been built in -Days days, or all of them with -All;
- folders under .claude\worktrees that git no longer knows as worktrees, when they hold
  nothing but build output (what a half-finished worktree removal leaves behind);
- old copies of the installed shell that scripts\install.ps1 set aside (cash.exe.old-*),
  once nothing runs them.

A folder is left alone while it is in use: while a program inside it runs, or while a
cargo build holds its lock. Anything else is renamed with a .cash-trash- suffix, which
is instant, and then deleted by a background process, so the script returns at once.
The installed shell itself (%USERPROFILE%\.cash-dev\cash.exe) is never touched, which is
what makes every target\ folder safe to delete.

Claude Code runs this at the start of every session (.claude\settings.json), with -Hook.

.EXAMPLE
powershell -File scripts\tidy.ps1 -DryRun -Report
Shows what would be deleted, and how big each checkout's build output is.

.EXAMPLE
powershell -File scripts\tidy.ps1 -All
Deletes every build folder that is not in use, whatever its age.
#>
[CmdletBinding()]
param(
    # Build folders not built for this many days are deleted.
    [int]$Days = 7,
    # Delete every build folder that is not in use, whatever its age.
    [switch]$All,
    # Warn when the drive holding the repository has less free space than this.
    [int]$MinFreeGB = 30,
    # Say what would be deleted, and delete nothing.
    [switch]$DryRun,
    # Also measure each checkout's build output (slow: it reads the size of every file).
    [switch]$Report,
    # For the SessionStart hook: print only deletions and warnings (Claude reads them), and
    # never fail the session.
    [switch]$Hook
)

$ErrorActionPreference = 'Stop'

$trashMarker = '.cash-trash-'
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$installDir = Join-Path $env:USERPROFILE '.cash-dev'

# Folders renamed for deletion, and one line per deletion for the summary.
$doomed = New-Object System.Collections.Generic.List[string]
$deleted = New-Object System.Collections.Generic.List[string]

# Detail printed only when run by hand.
function Write-Detail([string]$Text) {
    if (-not $Hook) { Write-Output $Text }
}

function Get-NormalPath([string]$Path) {
    [IO.Path]::GetFullPath($Path).TrimEnd('\').ToLowerInvariant()
}

# The newest time cargo wrote into a build folder. Cargo adds files to deps\ and
# .fingerprint\ on every compile, which updates those folders' own modification times,
# so a handful of stat calls is enough; no need to read the thousands of files inside.
function Get-LastBuild([string]$Dir) {
    $candidates = @($Dir) + @('deps', '.fingerprint', 'build', 'incremental' | ForEach-Object { Join-Path $Dir $_ })
    # A folder made with --target-dir holds profile folders of its own (swap\release\deps).
    $candidates += @(Get-ChildItem -LiteralPath $Dir -Directory -Force -ErrorAction SilentlyContinue |
            ForEach-Object { Join-Path $_.FullName 'deps'; Join-Path $_.FullName '.fingerprint' })
    $times = @($candidates | Where-Object { Test-Path -LiteralPath $_ } |
            ForEach-Object { (Get-Item -LiteralPath $_ -Force).LastWriteTime })
    ($times | Measure-Object -Maximum).Maximum
}

# The paths of every running program, to leave alone a folder one of them runs from.
$runningPaths = @(Get-Process | ForEach-Object {
        try { $_.Path } catch { $null }
    } | Where-Object { $_ } | ForEach-Object { $_.ToLowerInvariant() })

# Cargo holds a lock on .cargo-lock (and, since 1.9x, .cargo-build-lock and
# .cargo-artifact-lock) for as long as it builds. It opens the file sharing delete
# access, so renaming the folder would still succeed; the lock itself has to be tried.
function Test-CargoLocked([string]$LockFile) {
    try {
        $stream = [IO.File]::Open($LockFile, 'Open', 'ReadWrite', 'ReadWrite, Delete')
    } catch {
        return $true
    }
    try {
        $stream.Lock(0, 1)
        $stream.Unlock(0, 1)
        return $false
    } catch {
        return $true
    } finally {
        $stream.Close()
    }
}

function Test-InUse([string]$Dir) {
    $prefix = (Get-NormalPath $Dir) + '\'
    foreach ($path in $runningPaths) {
        if ($path.StartsWith($prefix)) { return $true }
    }
    # The lock is in the profile folder (debug\), or one level down in a folder made
    # with --target-dir, or two levels down when $Dir is a whole worktree's target\.
    $patterns = @('.cargo*lock', '*\.cargo*lock', 'target\*\.cargo*lock', 'target\*\*\.cargo*lock')
    foreach ($pattern in $patterns) {
        foreach ($lock in @(Get-ChildItem -Path (Join-Path $Dir $pattern) -File -Force -ErrorAction SilentlyContinue)) {
            if (Test-CargoLocked $lock.FullName) { return $true }
        }
    }
    return $false
}

function Remove-Later([string]$Dir, [string]$Why) {
    if (Test-InUse $Dir) {
        Write-Detail "in use, left alone: $Dir"
        return
    }
    if ($DryRun) {
        Write-Output "would delete $Dir ($Why)"
        return
    }
    $newName = (Split-Path -Leaf $Dir) + $trashMarker + $stamp
    try {
        Rename-Item -LiteralPath $Dir -NewName $newName
    } catch {
        Write-Detail "could not delete ${Dir}: $($_.Exception.Message)"
        return
    }
    $doomed.Add((Join-Path (Split-Path -Parent $Dir) $newName))
    $deleted.Add("$Dir ($Why)")
}

function Invoke-Tidy {
    $repo = Split-Path -Parent $PSScriptRoot
    $commonDir = git -C $repo rev-parse --path-format=absolute --git-common-dir
    if ($LASTEXITCODE -ne 0) { throw "not a git checkout: $repo" }
    $main = Split-Path -Parent ($commonDir -replace '/', '\')

    # Forget worktrees whose folders are gone, so the list below is the real one.
    if (-not $DryRun) {
        git -C $main worktree prune
    }
    $checkouts = @(git -C $main worktree list --porcelain |
            Where-Object { $_ -like 'worktree *' } |
            ForEach-Object { $_.Substring(9) -replace '/', '\' } |
            Where-Object { Test-Path -LiteralPath $_ })
    $registered = @($checkouts | ForEach-Object { Get-NormalPath $_ })

    # Leftovers under .claude\worktrees: folders git does not know, holding nothing but
    # build output, and anything an earlier run renamed but did not finish deleting.
    $worktreeRoot = Join-Path $main '.claude\worktrees'
    if (Test-Path -LiteralPath $worktreeRoot) {
        foreach ($dir in @(Get-ChildItem -LiteralPath $worktreeRoot -Directory -Force)) {
            if ($dir.Name.Contains($trashMarker)) {
                if (-not $DryRun) { $doomed.Add($dir.FullName) }
                continue
            }
            if ($registered -contains (Get-NormalPath $dir.FullName)) { continue }
            $contents = @(Get-ChildItem -LiteralPath $dir.FullName -Force | Where-Object { $_.Name -ne 'target' })
            if ($contents.Count -eq 0) {
                Remove-Later $dir.FullName 'not a worktree any more'
            } else {
                Write-Detail "left alone: $($dir.FullName) is not a worktree, but holds more than build output"
            }
        }
    }

    # Build folders.
    $cutoff = (Get-Date).AddDays(-$Days)
    foreach ($checkout in $checkouts) {
        $target = Join-Path $checkout 'target'
        if (-not (Test-Path -LiteralPath $target)) { continue }
        foreach ($dir in @(Get-ChildItem -LiteralPath $target -Directory -Force)) {
            if ($dir.Name.Contains($trashMarker)) {
                if (-not $DryRun) { $doomed.Add($dir.FullName) }
                continue
            }
            $last = Get-LastBuild $dir.FullName
            if ($All) {
                Remove-Later $dir.FullName 'all build output'
            } elseif ($last -lt $cutoff) {
                Remove-Later $dir.FullName ('last built ' + $last.ToString('yyyy-MM-dd'))
            }
        }
    }

    # Installed copies set aside by install.ps1, and those of the swap by hand it replaced.
    $oldCopies = @()
    if (Test-Path -LiteralPath $installDir) {
        $oldCopies += @(Get-ChildItem -LiteralPath $installDir -Filter 'cash.exe.old*' -File -Force)
    }
    foreach ($checkout in $checkouts) {
        $release = Join-Path $checkout 'target\release'
        if (Test-Path -LiteralPath $release) {
            $oldCopies += @(Get-ChildItem -LiteralPath $release -Filter 'cash.exe.old*' -File -Force)
        }
    }
    foreach ($copy in $oldCopies) {
        if ($DryRun) {
            Write-Output "would delete $($copy.FullName) (an old copy of cash.exe)"
            continue
        }
        try {
            Remove-Item -LiteralPath $copy.FullName -Force
            $deleted.Add("$($copy.FullName) (an old copy of cash.exe)")
        } catch {
            # Still running in an open tab; the next run gets it.
            Write-Detail "still running, left alone: $($copy.FullName)"
        }
    }

    # Delete in the background: a target\ folder can take minutes to delete. If this
    # process is stopped first, the .cash-trash- folders are picked up by the next run.
    if ($doomed.Count -gt 0) {
        $commands = @($doomed | ForEach-Object { "rd /s /q `"$_`"" }) -join ' & '
        Start-Process -FilePath $env:ComSpec -ArgumentList '/d', '/c', $commands -WindowStyle Hidden
    }

    if ($deleted.Count -gt 0) {
        Write-Output "tidy.ps1 deleted $($deleted.Count) unused build folder(s) or old cash.exe copies:"
        $deleted | ForEach-Object { Write-Output "  $_" }
    } elseif (-not $DryRun) {
        Write-Detail 'Nothing to delete.'
    }

    if ($Report) {
        foreach ($checkout in $checkouts) {
            $target = Join-Path $checkout 'target'
            if (-not (Test-Path -LiteralPath $target)) { continue }
            $bytes = (Get-ChildItem -LiteralPath $target -Recurse -File -Force -ErrorAction SilentlyContinue |
                    Measure-Object Length -Sum).Sum
            Write-Output ('{0,8:N1} GB  {1}' -f ($bytes / 1GB), $target)
        }
    }

    $drive = New-Object IO.DriveInfo ([IO.Path]::GetPathRoot($main))
    $freeGB = [math]::Floor($drive.AvailableFreeSpace / 1GB)
    if ($freeGB -lt $MinFreeGB) {
        Write-Output (("Drive {0} has only {1} GB free (warning below {2} GB). A cash build can take " +
                "10 GB or more. Run scripts\tidy.ps1 -All to delete every build folder not in use, " +
                "and avoid extra --target-dir folders and feature sets.") -f $drive.Name, $freeGB, $MinFreeGB)
    } else {
        Write-Detail "Drive $($drive.Name) has $freeGB GB free."
    }
}

if ($Hook) {
    # A failing hook would show an error at the start of every session; say it once
    # instead, and let the session start.
    try { Invoke-Tidy } catch { Write-Output "tidy.ps1 failed: $($_.Exception.Message)" }
    exit 0
}
Invoke-Tidy
