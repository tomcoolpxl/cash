param(
    [string]$Cash = "$PSScriptRoot/../../target/debug/cash.exe",
    [string]$Bash = 'C:/Program Files/Git/bin/bash.exe'
)
$ErrorActionPreference = 'Stop'
$cases = @(
    @{ name = 'local-options'; script = 'set +u; f() { local -; set -u; }; f; case $- in *u*) echo leaked;; *) echo restored;; esac' },
    @{ name = 'wait-job-status'; script = 'bash -c "exit 7" & wait %1; echo status:$?' },
    @{ name = 'wait-pid-status'; script = 'bash -c "exit 7" & wait $!; echo status:$?' },
    @{ name = 'wait-n'; script = 'bash -c "sleep 0.1; exit 7" & wait -n; echo status:$?' },
    @{ name = 'wait-f'; script = 'bash -c "exit 7" & wait -f $!; echo status:$?' },
    @{ name = 'wait-p'; script = 'bash -c "sleep 0.1; exit 7" & wait -n -p done; s=$?; echo status:$s assigned:${done:+yes}' },
    @{ name = 'mapfile-callback'; script = 'cb() { printf "callback:%s:%s\n" "$1" "$2"; }; mapfile -t -C cb -c 1 a <<< hello; printf "value:%s\n" "${a[0]}"' },
    @{ name = 'lastpipe-default'; script = 'v=before; echo after | read v; echo "$v"' },
    @{ name = 'lastpipe-enabled'; script = 'shopt -s lastpipe; v=before; echo after | read v; echo "$v"' },
    @{ name = 'errexit-condition'; script = 'set -e; f() { false; echo survived; }; if f; then echo yes; fi' },
    @{ name = 'errexit-substitution'; script = 'set -e; x=$(false; echo survived); echo "$x"' },
    @{ name = 'inherit-errexit'; script = 'set -e; shopt -s inherit_errexit; x=$(false; echo survived); echo unexpected' },
    @{ name = 'pipefail'; script = 'set -o pipefail; false | true; echo status:$?' },
    @{ name = 'pipestatus'; script = 'false | true; printf "%s\n" "${PIPESTATUS[*]}"' },
    @{ name = 'assignment-status'; script = 'a=$(false); echo assignment:$?; f() { local b=$(false); echo local:$?; }; f' },
    @{ name = 'dynamic-scope'; script = 'x=outer; g() { echo "$x"; }; f() { local x=inner; g; }; f; g' },
    @{ name = 'nameref'; script = 'x=old; declare -n r=x; r=new; echo "$x"; unset -n r; echo "$x"' },
    @{ name = 'integer-expression'; script = 'declare -i x; x="2+3*4"; echo "$x"' },
    @{ name = 'ifs-empty-fields'; script = 'IFS=:; x=":a::b:"; set -- $x; printf "<%s>\n" "$@"' },
    @{ name = 'quoted-empty-array'; script = 'a=(); set -- "${a[@]}"; echo "$#"' },
    @{ name = 'array-negative-index'; script = 'a=(one two three); echo "${a[-1]}"; a[-1]=last; echo "${a[@]}"' },
    @{ name = 'patsub-replacement'; script = 'x=abc; echo "${x/b/[&]}"' },
    @{ name = 'patsub-disabled'; script = 'shopt -u patsub_replacement; x=abc; echo "${x/b/[&]}"' },
    @{ name = 'patsub-dollar-literal'; script = 'x=abc; r=''$1''; echo "${x/b/$r}"' },
    @{ name = 'read-delimiter'; script = 'IFS= read -r -d : v <<< "a:b"; printf "status:%s value:%s\n" "$?" "$v"' },
    @{ name = 'read-reply'; script = 'read <<< "  hello  "; printf "<%s>\n" "$REPLY"' },
    @{ name = 'printf-count'; script = 'printf "abc%n\n" n; echo "$n"' },
    @{ name = 'command-lookup'; script = 'echo() { builtin echo function; }; echo; command echo builtin; builtin echo builtin' },
    @{ name = 'err-trap'; script = 'trap ''echo trapped:$?'' ERR; false; echo done' },
    @{ name = 'return-trap'; script = 'trap ''echo returned'' RETURN; f() { echo function; }; f' },
    @{ name = 'startup-bash-env'; script = 'printf "echo startup\n" > startup.sh; BASH_ENV=./startup.sh bash -c ''echo body''' },
    @{ name = 'startup-posix'; script = 'printf "echo startup\n" > startup.sh; BASH_ENV=./startup.sh bash --posix -c ''echo body''' },
    @{ name = 'startup-sh-env'; script = 'printf "echo startup\n" > startup.sh; ENV=./startup.sh sh -c ''echo body''' },
    @{ name = 'bash53-current-substitution'; script = 'x=old; y=${ x=new; printf value; }; echo "$x:$y"' },
    @{ name = 'bash53-compgen-V'; script = 'compgen -V a -W "one two"; printf "<%s>\n" "${a[@]}"' }
)
function Invoke-Probe([string]$Executable, [string]$Script) {
    $info = [System.Diagnostics.ProcessStartInfo]::new()
    $info.FileName = (Resolve-Path -LiteralPath $Executable).Path
    $scratch = "$PSScriptRoot/../../target/bash-reference-probes"
    New-Item -ItemType Directory -Path $scratch -Force | Out-Null
    $info.WorkingDirectory = (Resolve-Path -LiteralPath $scratch).Path
    $info.UseShellExecute = $false
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $info.RedirectStandardInput = $true
    foreach ($name in @('BASH_ENV', 'ENV', 'SHELLOPTS', 'BASHOPTS')) { $info.Environment.Remove($name) | Out-Null }
    foreach ($arg in @('--noprofile', '--norc', '-c', $Script)) { $info.ArgumentList.Add($arg) }
    $process = [System.Diagnostics.Process]::Start($info)
    $process.StandardInput.Close()
    $stdout = $process.StandardOutput.ReadToEndAsync()
    $stderr = $process.StandardError.ReadToEndAsync()
    if (-not $process.WaitForExit(10000)) {
        $process.Kill($true)
        throw "Probe timed out: $Script"
    }
    $result = @{ code = $process.ExitCode; stdout = $stdout.GetAwaiter().GetResult(); stderr = $stderr.GetAwaiter().GetResult() }
    $process.Dispose()
    return $result
}
$results = foreach ($case in $cases) {
    $reference = Invoke-Probe $Bash $case.script
    $actual = Invoke-Probe $Cash $case.script
    [pscustomobject]@{
        name = $case.name
        script = $case.script
        match = ($reference.code -eq $actual.code -and $reference.stdout -ceq $actual.stdout)
        bash = $reference
        cash = $actual
    }
}
$results | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath "$PSScriptRoot/results.json" -Encoding utf8
$results | Select-Object name, match | Format-Table
