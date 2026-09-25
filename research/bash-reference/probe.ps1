param(
    [string]$Cash = "$PSScriptRoot/../../target/debug/cash.exe",
    [string]$Bash = 'C:/Program Files/Git/bin/bash.exe',
    [ValidateSet('52', '53')]
    [string]$Suite = '52'
)
$ErrorActionPreference = 'Stop'
$cases52 = @(
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
# Bash 5.3 NEWS items (see bash-5.3-audit.md). Names carry the NEWS letter.
$cases53 = @(
    @{ name = '1a-shebang-binary-check'; script = 'printf ''#!/bin/sh\necho ran\n\0\n'' > nul.sh; "$BASH" ./nul.sh; echo rc=$?' },
    @{ name = '1c-unterminated-if-line'; script = 'printf ''echo a\n\nif true; then\n echo b\n'' > unterm.sh; "$BASH" ./unterm.sh 2>&1 | grep -o ''line [0-9]*'' | tail -n 1' },
    @{ name = '1d-posix-jobs-removes'; script = 'set -o posix; true & sleep 0.5; jobs >/dev/null; jobs | wc -l' },
    @{ name = '1g-regex-compile-error'; script = 're=''(''; err=$( { [[ a =~ $re ]]; } 2>&1 ); echo rc=$? ${err:+diagnostic}' },
    @{ name = '1h-umask-symbolic'; script = 'umask 022; umask -S; umask u=rwx,g=rx,o=; umask; umask a+w; umask; umask g-w,o-rwx; umask -S; umask -p' },
    @{ name = '1i-type-a-P-hashed'; script = 'hash -p /no/such/ls ls; type -a -P ls | head -n 1; type -P ls' },
    @{ name = '1j-trap-P'; script = 'trap ''echo hi'' INT; trap -P INT; trap ''echo bye'' EXIT; trap -P EXIT INT; trap - EXIT; trap -P TERM; echo rc=$?' },
    @{ name = '1j-trap-P-errors'; script = 'trap -P 2>/dev/null; echo none=$?; trap -p -P INT 2>/dev/null; echo both=$?' },
    @{ name = '1k-command-declare'; script = 'command declare a=$(echo x y); echo "$a"; command export b=$(echo p q); echo "$b"; command command readonly c=$(echo m n); echo "$c"' },
    @{ name = '1k-command-local'; script = 'f() { command local c=$(echo m n); echo "$c"; }; f' },
    @{ name = '1l-printf-alt-q'; script = 'printf ''<%#q>\n'' abc ''a b'' "it''s" ''''; printf ''<%q>\n'' ''a b''' },
    @{ name = '1l-printf-alt-Q'; script = 'printf ''<%#Q>\n'' ''a b''; printf ''<%.2Q>\n'' ''a b c''' },
    @{ name = '1q-globsort-name'; script = 'rm -rf gs; mkdir gs; cd gs; printf aaa > b; printf a > c; printf aa > a; echo *; GLOBSORT=-name; echo *; GLOBSORT=+name; echo *' },
    @{ name = '1q-globsort-size'; script = 'rm -rf gs; mkdir gs; cd gs; printf aaa > b; printf a > c; printf aa > a; GLOBSORT=size; echo *; GLOBSORT=-size; echo *' },
    @{ name = '1q-globsort-mtime'; script = 'rm -rf gs; mkdir gs; cd gs; touch -t 202001010000 a; touch -t 202101010000 b; touch -t 201901010000 c; GLOBSORT=mtime; echo *; GLOBSORT=-mtime; echo *' },
    @{ name = '1q-globsort-numeric'; script = 'rm -rf gs; mkdir gs; cd gs; touch 10 9 100 1; echo *; GLOBSORT=numeric; echo *; GLOBSORT=-numeric; echo *' },
    @{ name = '1q-globsort-invalid'; script = 'rm -rf gs; mkdir gs; cd gs; touch b a c; GLOBSORT=bogus; echo *; GLOBSORT=; echo *; unset GLOBSORT; echo *' },
    @{ name = '1t-array-expand-once-alias'; script = 'shopt array_expand_once; shopt -s assoc_expand_once; shopt array_expand_once; shopt -u array_expand_once; shopt assoc_expand_once' },
    @{ name = '1t-array-expand-once-unset'; script = 'shopt -s array_expand_once; declare -A a; k=''$(echo ran >&2)x''; a[$k]=v; unset ''a[$k]''; echo count=${#a[@]}' },
    @{ name = '1v-timeformat-precision'; script = 'TIMEFORMAT=''%6R|%3R|%0R|%9R''; x=$( { time sleep 0; } 2>&1 ); [[ $x =~ ^[0-9]+\.[0-9]{6}\|[0-9]+\.[0-9]{3}\|[0-9]+\|[0-9]+\.[0-9]{6}$ ]] && echo ok || echo "bad:$x"' },
    @{ name = '1w-monoseconds'; script = 'a=$BASH_MONOSECONDS; b=$BASH_MONOSECONDS; [[ $a =~ ^[0-9]+$ ]] && echo numeric; (( b >= a )) && echo monotonic; BASH_MONOSECONDS=5; [[ $BASH_MONOSECONDS != 5 ]] && echo dynamic' },
    @{ name = '1x-trapsig-exit'; script = 'trap ''echo sig=$BASH_TRAPSIG'' EXIT; echo out=${BASH_TRAPSIG-unset}' },
    @{ name = '1x-trapsig-err'; script = 'trap ''echo sig=$BASH_TRAPSIG'' ERR; false; true' },
    @{ name = '1x-trapsig-debug-return'; script = 'trap ''echo ret=$BASH_TRAPSIG'' RETURN; f() { :; }; f; trap - RETURN; trap ''echo dbg=$BASH_TRAPSIG; trap - DEBUG'' DEBUG; :' },
    @{ name = '1z-posix-test-collation'; script = 'set -o posix; [ a \< B ]; echo posix=$?; set +o posix; [ a \< B ]; echo default=$?' },
    @{ name = '1ee-test-parenthesized'; script = 'test \( a = a -a b = b \) -a \( x \); echo $?; test \( ! -n "" \) -o x = y; echo $?; test \( -n x \) -a \( -z "" \) -a y; echo $?' },
    @{ name = '1ff-multiple-coprocs'; script = 'coproc A { read l; echo A:$l; }; coproc B { read l; echo B:$l; }; echo 1 >&${A[1]}; echo 2 >&${B[1]}; read x <&${A[0]}; read y <&${B[0]}; echo $x $y' },
    @{ name = '1p-empty-path'; script = 'rm -rf ep; mkdir ep; cd ep; printf ''#!/bin/sh\necho ran\n'' > tool; chmod +x tool; PATH= tool; PATH=: tool; echo rc=$?' },
    @{ name = '1pp-bash-source-fullpath'; script = 'printf ''echo "${BASH_SOURCE[0]}"\n'' > src.sh; . ./src.sh; shopt -s bash_source_fullpath; s=$(. ./src.sh); [[ $s == */src.sh && $s != ./* ]] && echo full || echo "not-full:$s"' },
    @{ name = '1tt-posix-function-names'; script = 'set -o posix; f-1() { echo dash; }; f-1; a.b() { echo dot; }; a.b' },
    @{ name = '1uu-exit-in-trap'; script = 'trap ''false; exit'' EXIT; true' },
    @{ name = '1uu-exit-in-trap-status'; script = 'trap ''(exit 4); exit'' EXIT; exit 3' },
    @{ name = '1uu-exit-in-trap-subshell'; script = 'trap ''(false; exit); echo sub=$?'' EXIT; true' },
    @{ name = '1uu-exit-in-trap-function'; script = 'f() { false; exit; }; trap f EXIT; true' },
    @{ name = '1uu-exit-in-err-trap'; script = 'trap ''exit'' ERR; (exit 6); echo unreachable' }
)
$cases = if ($Suite -eq '53') { $cases53 } else { $cases52 }
$resultsFile = if ($Suite -eq '53') { 'results-53.json' } else { 'results.json' }
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
$results | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath "$PSScriptRoot/$resultsFile" -Encoding utf8
$results | Select-Object name, match | Format-Table
