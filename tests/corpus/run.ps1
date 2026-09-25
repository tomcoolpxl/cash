#!/usr/bin/env pwsh
<#
.SYNOPSIS
  Differential corpus runner: runs every case in tests/corpus/{sed,awk,bash} under the
  oracle (Git for Windows Bash 5.3 + GNU sed/gawk) and under cash, compares stdout
  (CRLF normalized to LF) and exit status, prints a table, and writes results.json.

.EXAMPLE
  pwsh tests/corpus/run.ps1
  pwsh tests/corpus/run.ps1 -Filter 'sed/pement-*'
  pwsh tests/corpus/run.ps1 -CheckArgPassing   # also runs oracle bash on the file itself, to prove script passing is lossless
#>
param(
    [string]$Cash = (Join-Path $PSScriptRoot '../../target/debug/cash.exe'),
    [string]$Bash = 'C:/Program Files/Git/bin/bash.exe',
    [string]$Filter = '*',
    [int]$TimeoutSec = 10,
    [int]$Throttle = 8,
    [string]$Json = (Join-Path $PSScriptRoot 'results.json'),
    [switch]$CheckArgPassing
)
$ErrorActionPreference = 'Stop'
$Cash = (Resolve-Path $Cash).Path
if (-not (Test-Path $Bash)) { throw "oracle bash not found: $Bash" }

$fixtures = Join-Path $PSScriptRoot 'fixtures'
$cases = foreach ($group in 'sed', 'awk', 'bash') {
    $dir = Join-Path $PSScriptRoot $group
    if (-not (Test-Path $dir)) { continue }
    Get-ChildItem $dir -Filter *.sh -File | Sort-Object Name | ForEach-Object {
        $rel = "$group/$($_.Name)"
        if ($rel -notlike $Filter -and $_.BaseName -notlike $Filter) { return }
        $text = [IO.File]::ReadAllText($_.FullName) -replace "`r`n", "`n"
        $meta = @{ source = ''; desc = ''; tags = '' }
        foreach ($l in $text -split "`n") {
            if ($l -notmatch '^#') { break }
            if ($l -match '^#\s*(source|desc|tags):\s*(.*)$') { $meta[$Matches[1]] = $Matches[2].Trim() }
        }
        [pscustomobject]@{
            name = $rel; group = $group; file = $_.FullName; script = $text
            source = $meta.source; desc = $meta.desc; tags = $meta.tags
        }
    }
}
if (-not $cases) { throw "no cases matched '$Filter'" }

$workRoot = Join-Path ([IO.Path]::GetTempPath()) ("cash-corpus-" + [guid]::NewGuid().ToString('N').Substring(0, 8))
New-Item -ItemType Directory $workRoot | Out-Null

# Git for Windows bash is an msys program: it re-parses the Windows command line with
# cygwin rules, which do not agree with the MSVC quoting .NET produces, so backslashes
# before quotes get mangled. The oracle therefore receives its script through an
# environment variable and a nested msys-to-msys exec, which passes argv verbatim:
#   bash -c 's=$CORPUS_SCRIPT; unset CORPUS_SCRIPT; exec "$BASH" --noprofile --norc -c "$s"'
# cash parses its command line with MSVC rules, so it gets the script directly.
$runner = {
    param($exe, [string[]]$argv, $cwd, $timeoutSec, $viaEnv)
    $psi = [System.Diagnostics.ProcessStartInfo]::new($exe)
    if ($null -ne $viaEnv) {
        $psi.Environment['CORPUS_SCRIPT'] = $viaEnv
        $argv = @('--noprofile', '--norc', '-c', 's=$CORPUS_SCRIPT; unset CORPUS_SCRIPT; exec "$BASH" --noprofile --norc -c "$s"')
    }
    foreach ($a in $argv) { $psi.ArgumentList.Add($a) }
    $psi.WorkingDirectory = $cwd
    $psi.UseShellExecute = $false
    $psi.RedirectStandardInput = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.StandardOutputEncoding = [System.Text.UTF8Encoding]::new($false)
    $psi.StandardErrorEncoding = [System.Text.UTF8Encoding]::new($false)
    $psi.Environment['LC_ALL'] = 'C'
    $p = [System.Diagnostics.Process]::Start($psi)
    $p.StandardInput.Close()   # stdin closed: reads see EOF
    $out = $p.StandardOutput.ReadToEndAsync()
    $err = $p.StandardError.ReadToEndAsync()
    $status = $null
    if ($p.WaitForExit($timeoutSec * 1000)) { $p.WaitForExit(); $status = $p.ExitCode }
    else { try { $p.Kill($true) } catch {}; $status = 'timeout' }
    $o = ''; $e = ''
    if ($out.Wait(2000)) { $o = $out.Result }
    if ($err.Wait(2000)) { $e = $err.Result }
    [pscustomobject]@{
        stdout = $o -replace "`r`n", "`n"
        stderr = $e -replace "`r`n", "`n"
        status = $status
    }
}.ToString()

$results = $cases | ForEach-Object -ThrottleLimit $Throttle -Parallel {
    $c = $_
    $run = [scriptblock]::Create($using:runner)
    $safe = $c.name -replace '[\\/]', '__'
    $res = [ordered]@{}
    $shells = [ordered]@{ bash = $using:Bash; cash = $using:Cash }
    foreach ($k in $shells.Keys) {
        $cwd = Join-Path $using:workRoot "$safe/$k"
        New-Item -ItemType Directory -Force $cwd | Out-Null
        if (Test-Path $using:fixtures) { Copy-Item (Join-Path $using:fixtures '*') $cwd -Recurse }
        $viaEnv = if ($k -eq 'bash') { $c.script } else { $null }
        $res[$k] = & $run $shells[$k] @('--noprofile', '--norc', '-c', $c.script) $cwd $using:TimeoutSec $viaEnv
    }
    $argCheck = $null
    if ($using:CheckArgPassing) {
        $cwd = Join-Path $using:workRoot "$safe/bashfile"
        New-Item -ItemType Directory -Force $cwd | Out-Null
        if (Test-Path $using:fixtures) { Copy-Item (Join-Path $using:fixtures '*') $cwd -Recurse }
        $f = & $run $using:Bash @('--noprofile', '--norc', ($c.file -replace '\\', '/')) $cwd $using:TimeoutSec $null
        $argCheck = ($f.stdout -ceq $res.bash.stdout) -and ("$($f.status)" -eq "$($res.bash.status)")
        $cwd = Join-Path $using:workRoot "$safe/cashfile"
        New-Item -ItemType Directory -Force $cwd | Out-Null
        if (Test-Path $using:fixtures) { Copy-Item (Join-Path $using:fixtures '*') $cwd -Recurse }
        $g = & $run $using:Cash @('--noprofile', '--norc', $c.file) $cwd $using:TimeoutSec $null
        $argCheck = $argCheck -and ($g.stdout -ceq $res.cash.stdout) -and ("$($g.status)" -eq "$($res.cash.status)")
    }
    $outMatch = $res.bash.stdout -ceq $res.cash.stdout
    $stMatch = "$($res.bash.status)" -eq "$($res.cash.status)"
    [pscustomobject]@{
        name = $c.name; group = $c.group; source = $c.source; desc = $c.desc; tags = $c.tags
        script = $c.script; match = ($outMatch -and $stMatch)
        stdoutMatch = $outMatch; statusMatch = $stMatch; argPassingOk = $argCheck
        bash = $res.bash; cash = $res.cash
    }
} | Sort-Object name

Remove-Item -Recurse -Force $workRoot -ErrorAction SilentlyContinue

$results | ForEach-Object {
    $why = if ($_.match) { '' } else {
        @(if (-not $_.stdoutMatch) { 'stdout' }; if (-not $_.statusMatch) { "status $($_.bash.status)/$($_.cash.status)" }) -join ', '
    }
    [pscustomobject]@{ case = $_.name; match = if ($_.match) { 'PASS' } else { 'FAIL' }; diff = $why; tags = $_.tags }
} | Format-Table -AutoSize | Out-String -Width 200 | Write-Host

$summary = foreach ($g in 'sed', 'awk', 'bash') {
    $r = @($results | Where-Object group -eq $g)
    if ($r.Count) { [pscustomobject]@{ group = $g; cases = $r.Count; pass = @($r | Where-Object match).Count; fail = @($r | Where-Object { -not $_.match }).Count } }
}
$summary | Format-Table -AutoSize | Out-String | Write-Host
if ($CheckArgPassing) {
    $bad = @($results | Where-Object { $_.argPassingOk -eq $false })
    Write-Host "arg-passing check: $($bad.Count) case(s) where '<shell> -c <text>' differs from '<shell> <file>'"
    $bad | ForEach-Object { Write-Host "  $($_.name)" }
}

$report = [ordered]@{
    generated = (Get-Date).ToString('o')
    oracle = $Bash; cash = $Cash
    summary = $summary
    mismatches = @($results | Where-Object { -not $_.match } | ForEach-Object {
            [ordered]@{
                name = $_.name; source = $_.source; desc = $_.desc; tags = $_.tags; script = $_.script
                stdoutMatch = $_.stdoutMatch; statusMatch = $_.statusMatch
                bash = $_.bash; cash = $_.cash
            }
        })
}
$report | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8NoBOM $Json
Write-Host "wrote $Json"
if (@($results | Where-Object { -not $_.match }).Count) { exit 1 }
