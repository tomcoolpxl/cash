<#
Lists the tests nextest passed only on a retry: in the job's summary, and as a warning
on the run. CI retries a failed test (.config/nextest.toml), so a flaky test would
otherwise pass unseen. nextest's JUnit file marks each failed try of a test that then
passed with a <flakyFailure> element.
#>
param(
    [Parameter(Mandatory)]
    [string] $Results
)

if (-not (Test-Path -LiteralPath $Results)) {
    Write-Host "No test results at $Results."
    exit 0
}

[xml] $xml = Get-Content -Raw -LiteralPath $Results
$flaky = @($xml.SelectNodes('//testcase[flakyFailure]'))
if ($flaky.Count -eq 0) {
    Write-Host 'No flaky tests.'
    exit 0
}

$lines = @('### Flaky tests', '', 'Each failed at least once and passed on a retry:', '')
foreach ($case in $flaky) {
    $name = "$($case.classname) $($case.name)"
    $tries = $case.SelectNodes('flakyFailure').Count
    Write-Host "::warning title=Flaky test::$name failed $tries time(s) before it passed"
    $lines += "- ``$name``, $tries failed tr$(if ($tries -eq 1) { 'y' } else { 'ies' })"
}

if ($env:GITHUB_STEP_SUMMARY) {
    $lines | Out-File -FilePath $env:GITHUB_STEP_SUMMARY -Append -Encoding utf8
} else {
    $lines | Write-Host
}
