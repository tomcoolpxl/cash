# Writes a release's notes from its commits, grouped by Conventional Commits type.
#
#   pwsh .github/scripts/release-notes.ps1 -Tag v0.9.2 [-Out release-notes.md]
#
# The range runs from the tag before -Tag (or before HEAD, if -Tag does not exist yet,
# which is how to preview the next release) to -Tag. A subject such as
# "fix(kill): stop TERM from reaching the console" lands under "Bug fixes" as
# "**kill:** stop TERM from reaching the console"; a "!" before the colon also lists it
# under "Breaking changes". Subjects without a known type go under "Other", so history
# from before the convention still appears. See RELEASING.md, "Commit messages".

param(
    [Parameter(Mandatory)][string]$Tag,
    [string]$Out = 'release-notes.md'
)

$ErrorActionPreference = 'Stop'

if (git rev-parse -q --verify "refs/tags/$Tag") {
    # A release: from the tag before this one.
    $head = $Tag
    $previous = git describe --tags --abbrev=0 "$Tag^" 2>$null
} else {
    # A preview: everything since the latest tag, which HEAD itself may carry.
    $head = 'HEAD'
    $previous = git describe --tags --abbrev=0 HEAD 2>$null
}
if ($LASTEXITCODE -ne 0 -or -not $previous) {
    $range = $head
    $title = '## Changes'
} else {
    $range = "$previous..$head"
    $title = "## Changes since $previous"
}

# Section order, and the types that feed each section.
$sections = [ordered]@{
    'Features'      = @('feat')
    'Bug fixes'     = @('fix')
    'Performance'   = @('perf')
    'Documentation' = @('docs')
    'Refactoring'   = @('refactor')
    'Maintenance'   = @('chore', 'ci', 'test', 'build', 'style')
}
$sectionOf = @{}
foreach ($name in $sections.Keys) {
    foreach ($type in $sections[$name]) { $sectionOf[$type] = $name }
}

$entries = [ordered]@{ 'Breaking changes' = @() }
foreach ($name in $sections.Keys) { $entries[$name] = @() }
$entries['Other'] = @()

$pattern = '^(?<type>[a-z]+)(?:\((?<scope>[^)]+)\))?(?<bang>!)?:\s*(?<desc>.+)$'
$log = git log --no-merges --reverse --format='%h%x09%s' $range
foreach ($line in $log) {
    if (-not $line) { continue }
    $hash, $subject = $line -split "`t", 2
    $match = [regex]::Match($subject, $pattern)
    if ($match.Success -and $sectionOf.ContainsKey($match.Groups['type'].Value)) {
        # Kept as written: a description often starts with a command name (`ps`, `ls`),
        # which capitalising would change.
        $desc = $match.Groups['desc'].Value
        $scope = $match.Groups['scope'].Value
        $text = if ($scope) { "**${scope}:** $desc" } else { $desc }
        $entry = "- $text ($hash)"
        $entries[$sectionOf[$match.Groups['type'].Value]] += $entry
        if ($match.Groups['bang'].Success) { $entries['Breaking changes'] += $entry }
    } else {
        $entries['Other'] += "- $subject ($hash)"
    }
}

$body = @($title, '')
foreach ($name in $entries.Keys) {
    if ($entries[$name].Count -eq 0) { continue }
    $body += "### $name"
    $body += ''
    $body += $entries[$name]
    $body += ''
}
if ($body.Count -eq 2) { $body += @('- No changes recorded.', '') }

# The three ways to install, under every release's notes.
$version = $Tag -replace '^v', ''
$body += @(
    '## Install',
    '',
    "- **Portable**: unpack ``cash-$Tag-x86_64-pc-windows-msvc.zip`` anywhere and run ``cash.exe``. That one file is the whole shell; the licence notices are beside it.",
    "- **Installer**, per user, no admin: ``cash-$Tag-setup.exe``. Silent: ``/VERYSILENT /SUPPRESSMSGBOXES /NORESTART``. Unsigned, so a browser download shows SmartScreen once. Upgrade later with ``cash --update``.",
    '- **Scoop**: `scoop bucket add tomcoolpxl https://github.com/tomcoolpxl/scoop-bucket` then `scoop install cash`.',
    '',
    "Each ``.sha256`` file holds the asset's checksum. ``cash help installing`` has the details.",
    ''
)

$body | Set-Content -Path $Out -Encoding utf8
Get-Content $Out
