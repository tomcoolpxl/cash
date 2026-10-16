<#
Opens, or brings up to date, the one issue labelled `upstreams` with what the weekly
Upstreams workflow found: RustSec advisories against Cargo.lock (advisories.txt) and the
upstreams cash took in that moved on (upstreams.md, from `cargo xtask upstreams`). The
issue is edited, not opened again, so it holds this week's state; it is closed by hand
once nothing is left, and the next finding opens a new one.
#>
param(
    [Parameter(Mandatory)]
    [string] $Advisories,
    [Parameter(Mandatory)]
    [string] $Upstreams
)

$fence = '```'
$run = "$env:GITHUB_SERVER_URL/$env:GITHUB_REPOSITORY/actions/runs/$env:GITHUB_RUN_ID"
$lines = @("Found by the weekly Upstreams workflow ([this run]($run)).", '')

if ($Upstreams -eq 'failure') {
    $lines += '## Upstreams cash took in as its own', ''
    if (Test-Path -LiteralPath upstreams.md) {
        $lines += (Get-Content -Raw -LiteralPath upstreams.md)
        $advice = 'Port what matters, or mark how far upstream was looked through ' +
            '(`reviewed` in `upstreams.toml`). `cargo xtask upstreams` shows the same.'
        $lines += '', $advice
    } else {
        $lines += 'The check itself failed (crates.io or GitHub did not answer); see the run.'
    }
    $lines += ''
}

if ($Advisories -eq 'failure') {
    $lines += '## RustSec advisories against Cargo.lock', ''
    $lines += 'Update the crates named (`cargo update -p NAME`), as `deny.toml` says.', ''
    $lines += $fence + 'text'
    if (Test-Path -LiteralPath advisories.txt) {
        $lines += (Get-Content -Raw -LiteralPath advisories.txt)
    }
    $lines += $fence
}

$body = Join-Path $env:RUNNER_TEMP 'upstreams-issue.md'
Set-Content -LiteralPath $body -Value ($lines -join "`n") -Encoding utf8

gh label create upstreams --color FBCA04 `
    --description 'An upstream moved on, or an advisory names a dependency' --force
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$existing = gh issue list --label upstreams --state open --json number --jq '.[0].number'
if ($existing) {
    gh issue edit $existing --body-file $body
} else {
    gh issue create --title 'Upstreams or advisories want looking at' --label upstreams `
        --body-file $body
}
exit $LASTEXITCODE
