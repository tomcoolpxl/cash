# Renders the README's demo clips: assets\demos\*.tape -> assets\demos\*.gif, through VHS,
# then gifsicle shrinks them. Needs vhs, ttyd, ffmpeg, gifsicle and Scoop's chromium on
# this machine (`scoop install vhs ttyd ffmpeg gifsicle extras/chromium`), the released
# cash on PATH, and the clips' font, the Nerd Font build of Cascadia Mono
# (`scoop bucket add nerd-fonts; scoop install nerd-fonts/CascadiaMono-NF`).
#
#   powershell -File scripts\render-demos.ps1            # every tape
#   powershell -File scripts\render-demos.ps1 paths top  # these two
param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Only)
$ErrorActionPreference = 'Stop'

$root = Split-Path $PSScriptRoot -Parent
$demos = Join-Path $root 'assets\demos'

# VHS takes the first `chrome` on PATH, else Edge. Edge's process hands off to another
# as it starts, which VHS 0.12 takes for a crash; Scoop's chromium ships a chrome.exe
# that stays, so it goes first on this script's PATH.
$chromium = Join-Path $HOME 'scoop\apps\chromium\current'
if (Test-Path (Join-Path $chromium 'chrome.exe')) { $env:PATH = "$chromium;$env:PATH" }
foreach ($tool in 'vhs', 'ttyd', 'ffmpeg', 'gifsicle', 'cash', 'chrome') {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) { throw "$tool is not on PATH" }
}

Push-Location $demos
try {
    $tapes = Get-ChildItem *.tape | Where-Object { $_.Name -notlike '_*' }
    if ($Only) { $tapes = $tapes | Where-Object { $Only -contains $_.BaseName } }
    foreach ($tape in $tapes) {
        Write-Host "== $($tape.Name)"
        # Each clip starts from the same tree: the ports clip deletes a log, the
        # archivers clip leaves archives behind.
        & cash setup.sh
        if ($LASTEXITCODE -ne 0) { throw 'setup.sh failed' }
        & vhs $tape.Name
        if ($LASTEXITCODE -ne 0) { throw "vhs failed on $($tape.Name)" }
        $gif = $tape.BaseName + '.gif'
        & gifsicle -O3 --lossy=40 --colors 128 -o $gif $gif
        if ($LASTEXITCODE -ne 0) { throw "gifsicle failed on $gif" }
        '{0,-18} {1,6:n0} KB' -f $gif, ((Get-Item $gif).Length / 1KB)
    }
} finally {
    Pop-Location
    Remove-Item -Recurse -Force (Join-Path $env:PUBLIC 'cash-demo') -ErrorAction SilentlyContinue
}
