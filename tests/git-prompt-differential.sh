#!/usr/bin/env bash
# Differential test of Git's prompt script, git-prompt.sh (`__git_ps1`), and of Git for
# Windows' /etc/profile.d/git-prompt.sh, which also sources git-completion.bash. Each case
# runs under Git Bash (the oracle) and under cash, in fixture repositories covering every
# state __git_ps1 reports, and stdout and exit status must match byte for byte. Where
# cash differs on purpose, the case names the spec decision and checks cash's own output.
#
#   bash tests/git-prompt-differential.sh [oracle-bash] [cash] [case-glob]
#   bash tests/git-prompt-differential.sh --freeze [oracle-bash]
#   bash tests/git-prompt-differential.sh --check [cash]
#
# --freeze writes the oracle's output and status for each case into tests/git-prompt/,
# with the hashes of the scripts it came from; --check runs cash alone against them, and
# is what `it/git_prompt_goldens.rs` runs (BIN-05). When Git for Windows updates its
# scripts, --check says so: re-freeze, and look at what changed.
#
# Run it from Git Bash on Windows; it finds the scripts through `git --exec-path`.
set -uo pipefail

mode=compare
case ${1-} in
--freeze | --check) mode=${1#--}; shift ;;
esac
case $mode in
compare) oracle=${1:-bash} cash=${2:-target/debug/cash} filter=${3:-*} ;;
freeze) oracle=${1:-bash} cash= filter='*' ;;
check) oracle= cash=${1:-target/debug/cash} filter='*' ;;
esac
G=$(cd "$(dirname "$0")" && pwd)/git-prompt

git_exec=$(git --exec-path) || exit 2
GP=${git_exec%/libexec/git-core}/share/git/completion/git-prompt.sh
GC=${git_exec%/libexec/git-core}/share/git/completion/git-completion.bash
PD=${git_exec%/mingw64/libexec/git-core}/etc/profile.d/git-prompt.sh
if [[ ! -f $GP ]]; then
    printf 'git-prompt.sh not found at %s\n' "$GP" >&2
    exit 2
fi

# The scripts the oracle's output came from, by hash.
sources() {
    local label f
    for label in completion/git-prompt.sh completion/git-completion.bash profile.d/git-prompt.sh; do
        case $label in
        completion/git-prompt.sh) f=$GP ;;
        completion/*) f=$GC ;;
        profile.d/*) f=$PD ;;
        esac
        if [[ -f $f ]]; then
            printf '%s  %s\n' "$(sha256sum <"$f" | cut -d' ' -f1)" "$label"
        else
            printf 'missing  %s\n' "$label"
        fi
    done
}
if [[ $mode == check ]] && ! diff <(sources) "$G/SOURCES" >/dev/null 2>&1; then
    printf 'the installed scripts are not the ones tests/git-prompt was frozen from:\n' >&2
    diff <(sources) "$G/SOURCES" >&2
    printf 're-freeze with `bash tests/git-prompt-differential.sh --freeze` and look at what changed\n' >&2
    exit 3
fi

W=$(cygpath -m "$(mktemp -d)")
trap 'rm -rf "$W"' EXIT

# --- fixture repositories, one per state
setup() {
    set -e
    export GIT_CONFIG_GLOBAL="$W/gitconfig" GIT_CONFIG_NOSYSTEM=1
    export GIT_AUTHOR_DATE='2026-01-01T00:00:00Z' GIT_COMMITTER_DATE='2026-01-01T00:00:00Z'
    git config --global user.name T
    git config --global user.email t@example.com
    git config --global init.defaultBranch main
    git config --global core.autocrlf false
    git config --global advice.detachedHead false
    git config --global core.editor true
    mkdir -p "$W/repos"
    cd "$W/repos"
    mkdir norepo

    git init -q clean
    (
        cd clean
        mkdir -p sub/deeper
        echo keep >sub/b.txt
        echo keep >sub/deeper/c.txt
        echo one >a.txt
        git add .
        git commit -qm c1
        git tag -a v1.0 -m v1.0
        echo two >>a.txt; git commit -qam c2
        echo three >>a.txt; git commit -qam c3
    )
    mk() { git clone -q clean "$1"; }

    mk dirty-w; echo x >>dirty-w/a.txt
    mk dirty-i; (cd dirty-i && echo x >>a.txt && git add a.txt)
    mk dirty-wi; (cd dirty-wi && echo x >>a.txt && git add a.txt && echo y >>a.txt)
    mk untracked; : >untracked/sub/new.txt
    mk stash; (cd stash && echo x >>a.txt && git stash -q)
    mk allflags
    (cd allflags && echo s >>a.txt && git stash -q && echo x >>a.txt && git add a.txt &&
        echo y >>a.txt && : >new.txt)
    git init -q empty
    git init -q empty-staged; (cd empty-staged && : >f && git add f)
    mk detached-tag; (cd detached-tag && git checkout -q v1.0)
    mk detached-notag; (cd detached-notag && git checkout -q HEAD~1)

    git clone -q --bare clean origin.git
    for n in equal ahead behind diverged cfg; do git clone -q origin.git "up-$n"; done
    (cd up-ahead && echo a >>a.txt && git commit -qam ahead)
    (cd up-behind && git reset -q --hard HEAD~1)
    (cd up-diverged && git reset -q --hard HEAD~1 && echo d >>a.txt && git commit -qam d1 &&
        echo e >>a.txt && git commit -qam d2)
    (cd up-cfg && echo a >>a.txt && git commit -qam ahead &&
        git config bash.showUpstream 'verbose name')

    mk merge
    (
        cd merge
        git checkout -qb other
        echo other >a.txt; git commit -qam other
        git checkout -q main
        echo mine >a.txt; git commit -qam mine
        git merge -q other >/dev/null 2>&1 || true
    )
    mk rebase
    (
        cd rebase
        git checkout -qb topic HEAD~1
        echo topic >a.txt; git commit -qam t1
        echo more >t.txt; git add t.txt; git commit -qm t2
        git rebase main >/dev/null 2>&1 || true
    )
    mk cherry
    (
        cd cherry
        git checkout -qb pick HEAD~1
        echo pick >a.txt; git commit -qam pick
        git checkout -q main
        git cherry-pick pick >/dev/null 2>&1 || true
    )
    mk revert; (cd revert && { git revert --no-edit HEAD~1 >/dev/null 2>&1 || true; })
    mk bisect; (cd bisect && git bisect start >/dev/null)

    # States that are awkward to reach with real commands are written by hand.
    todo() { mk "$1"; mkdir "$1/.git/sequencer"; printf "$2" >"$1/.git/sequencer/todo"; }
    todo seq-pick 'pick abc1234 x\n'
    todo seq-revert 'revert abc1234 x\n'
    todo seq-p 'p\t\n'
    todo seq-crlf 'pick abc1234 x\r\n'
    mk am-applying; mkdir am-applying/.git/rebase-apply
    printf '2\n' >am-applying/.git/rebase-apply/next
    printf '5\n' >am-applying/.git/rebase-apply/last
    : >am-applying/.git/rebase-apply/applying
    mk am-rebasing; mkdir am-rebasing/.git/rebase-apply
    printf '3\n' >am-rebasing/.git/rebase-apply/next
    printf '4\n' >am-rebasing/.git/rebase-apply/last
    : >am-rebasing/.git/rebase-apply/rebasing
    printf 'refs/heads/feature\n' >am-rebasing/.git/rebase-apply/head-name
    mk am-neither; mkdir am-neither/.git/rebase-apply
    mk crlf-rebase; mkdir crlf-rebase/.git/rebase-merge
    printf 'refs/heads/crlf\r\n' >crlf-rebase/.git/rebase-merge/head-name
    printf '1\r\n' >crlf-rebase/.git/rebase-merge/msgnum
    printf '3\r\n' >crlf-rebase/.git/rebase-merge/end

    git init -q --bare bare.git
    mk sparse; (cd sparse && git sparse-checkout set sub)
    mk ignored
    (cd ignored && echo 'build/' >.gitignore && git add .gitignore && git commit -qm i && mkdir build)
    mk cfg-nohide
    (cd cfg-nohide && echo 'build/' >.gitignore && git add .gitignore && git commit -qm i &&
        mkdir build && git config bash.hideIfPwdIgnored false)
    mk wtmain; (cd wtmain && git worktree add -q ../wt -b wtbranch)
    git init -q --ref-format=reftable reftable
    (cd reftable && : >f && git add f && git commit -qm r)
    mk evil; (cd evil && git checkout -qb 'x$(echo-pwned)`echo-pwned2`')
    mk percent; (cd percent && git checkout -qb 'p%s%d')
    mk cfg-nodirty; (cd cfg-nodirty && echo x >>a.txt && git config bash.showDirtyState false)
    mk cfg-nountracked
    (cd cfg-nountracked && : >new.txt && git config bash.showUntrackedFiles false)
    # Cases run at the same time; this one writes into its repository.
    mk pd-config
}
if ! (setup) >"$W/setup.log" 2>&1; then
    cat "$W/setup.log" >&2
    exit 2
fi
printf 'fixtures made in %ds\n' "$SECONDS"

# t NAME REPO BODY [CASH-OUTPUT DECISION]
# Writes a case that runs BODY in $W/repos/REPO with git-prompt.sh sourced; they run
# together, at the end. With CASH-OUTPUT, cash differs on purpose (DECISION says where)
# and must print exactly that.
order=()
declare -A want why
t() {
    local name=$1 repo=$2 body=$3
    [[ $name == $filter ]] || return 0
    mkdir -p "$W/cases"
    # GIT_OPTIONAL_LOCKS=0: cases run at the same time, in the same repositories, and
    # `git status` would otherwise refresh the index under a lock.
    printf '%s\n' \
        "export GIT_CONFIG_GLOBAL='$W/gitconfig' GIT_CONFIG_NOSYSTEM=1 LC_ALL=C GIT_OPTIONAL_LOCKS=0" \
        "unset \${!GIT_PS1_*} PROMPT_COMMAND" \
        "cd '$W' && HOME=\$PWD" \
        ". '$GP'" \
        "cd '$W/repos/$repo' || exit 99" \
        "$body" >"$W/cases/$name.sh"
    order+=("$name")
    if (($# > 3)); then want[$name]=$4 why[$name]=$5; fi
}

# run NAME: one case in the mode asked for; PASS or FAIL on stdout.
run() {
    local name=$1 f="$W/cases/$1.sh" out="$W/out/$1" bs cs
    if [[ $mode != check ]]; then
        "$oracle" --noprofile --norc "$f" >"$out.bash" 2>/dev/null; bs=$?
    else
        if [[ ! -f $G/$name.out ]]; then
            printf 'FAIL %s\n  not frozen: re-freeze\n' "$name"
            return
        fi
        cp "$G/$name.out" "$out.bash"; bs=$(<"$G/$name.status")
    fi
    if [[ $mode == freeze ]]; then
        cp "$out.bash" "$G/$name.out"
        printf '%s\n' "$bs" >"$G/$name.status"
        printf 'FROZE %s\n' "$name"
        return
    fi
    "$cash" --noprofile --norc "$f" >"$out.cash" 2>/dev/null; cs=$?
    if [[ -v want[$name] ]]; then
        if [[ $cs == 0 && $(<"$out.cash") == "${want[$name]}" ]]; then
            printf 'PASS %s (differs on purpose, %s)\n' "$name" "${why[$name]}"
            return
        fi
    elif [[ $bs == "$cs" ]] && cmp -s "$out.bash" "$out.cash"; then
        printf 'PASS %s\n' "$name"
        return
    fi
    printf 'FAIL %s\n  bash[%s]=<%s>\n  cash[%s]=<%s>\n' "$name" \
        "$bs" "$(cat -v "$out.bash")" "$cs" "$(cat -v "$out.cash")"
}

# --- load-time globals
t globals clean 'declare -p __git_printf_supports_v __git_SOH __git_STX __git_ESC __git_LF __git_CRLF'
t functions clean 'declare -F | grep __git'

# --- basic
t norepo norepo 'false; __git_ps1; echo "<$?>"'
t clean clean '__git_ps1; echo "<$?>"'
t clean-fmt clean '__git_ps1 "[%s]"; echo'
t clean-fmt-empty clean '__git_ps1 ""; echo'
t clean-subdir clean 'cd sub/deeper && __git_ps1; echo'
t exit-preserve clean '(exit 42); __git_ps1 >/dev/null; echo "<$?>"'
t exit-preserve-norepo norepo '(exit 7); __git_ps1; echo "<$?>"'
t too-many-args clean '(exit 3); __git_ps1 a b c d; echo "<$?>" "$PS1"'
t cmdsub clean 'x=$(__git_ps1 " (%s)"); echo "[$x]"'
t backticks clean 'x=`__git_ps1`; echo "[$x]"'

# --- dirty, untracked, stash
t dirty-w-off dirty-w '__git_ps1; echo'
t dirty-w dirty-w 'GIT_PS1_SHOWDIRTYSTATE=1; __git_ps1; echo'
t dirty-i dirty-i 'GIT_PS1_SHOWDIRTYSTATE=1; __git_ps1; echo'
t dirty-wi dirty-wi 'GIT_PS1_SHOWDIRTYSTATE=1; __git_ps1; echo'
t dirty-prefix-assign dirty-wi 'GIT_PS1_SHOWDIRTYSTATE=1 __git_ps1; echo; echo "after=${GIT_PS1_SHOWDIRTYSTATE-unset}"'
t dirty-clean clean 'GIT_PS1_SHOWDIRTYSTATE=1; __git_ps1; echo'
t cfg-nodirty cfg-nodirty 'GIT_PS1_SHOWDIRTYSTATE=1; __git_ps1; echo'
t untracked untracked 'GIT_PS1_SHOWUNTRACKEDFILES=1; __git_ps1; echo'
t untracked-subdir untracked 'cd sub/deeper && GIT_PS1_SHOWUNTRACKEDFILES=1 && __git_ps1; echo'
t untracked-none clean 'GIT_PS1_SHOWUNTRACKEDFILES=1; __git_ps1; echo'
t cfg-nountracked cfg-nountracked 'GIT_PS1_SHOWUNTRACKEDFILES=1; __git_ps1; echo'
t stash stash 'GIT_PS1_SHOWSTASHSTATE=1; __git_ps1; echo'
t allflags allflags 'GIT_PS1_SHOWDIRTYSTATE=1 GIT_PS1_SHOWSTASHSTATE=1 GIT_PS1_SHOWUNTRACKEDFILES=1; __git_ps1; echo'
t sep-empty allflags 'GIT_PS1_SHOWDIRTYSTATE=1 GIT_PS1_SHOWUNTRACKEDFILES=1 GIT_PS1_STATESEPARATOR=; __git_ps1; echo'
t sep-bar allflags 'GIT_PS1_SHOWDIRTYSTATE=1 GIT_PS1_STATESEPARATOR="|"; __git_ps1; echo'
t empty empty '__git_ps1; echo'
t empty-dirty empty 'GIT_PS1_SHOWDIRTYSTATE=1; __git_ps1; echo'
t empty-staged empty-staged 'GIT_PS1_SHOWDIRTYSTATE=1; __git_ps1; echo'

# --- detached HEAD
t detached-tag detached-tag '__git_ps1; echo'
t detached-notag detached-notag '__git_ps1; echo'
for style in contains branch tag describe default bogus; do
    t "describe-$style" detached-notag "GIT_PS1_DESCRIBE_STYLE=$style; __git_ps1; echo"
done

# --- upstream; the verbose forms take `rev-list --count`'s tab-separated counts apart
for r in equal ahead behind diverged; do
    t "up-auto-$r" "up-$r" 'GIT_PS1_SHOWUPSTREAM=auto; __git_ps1; echo'
    t "up-verbose-$r" "up-$r" 'GIT_PS1_SHOWUPSTREAM="verbose name"; __git_ps1; echo; echo "name=${__git_ps1_upstream_name-unset}"'
    t "up-legacy-$r" "up-$r" 'GIT_PS1_SHOWUPSTREAM="legacy verbose"; __git_ps1; echo'
done
t up-git up-ahead 'GIT_PS1_SHOWUPSTREAM=git; __git_ps1; echo'
t up-none clean 'GIT_PS1_SHOWUPSTREAM=auto; __git_ps1; echo'
t up-cfg up-cfg 'GIT_PS1_SHOWUPSTREAM=auto; __git_ps1; echo; echo "$GIT_PS1_SHOWUPSTREAM"'

# --- operations in progress
t merge merge '__git_ps1; echo'
t merge-conflict merge 'GIT_PS1_SHOWCONFLICTSTATE=yes GIT_PS1_SHOWDIRTYSTATE=1; __git_ps1; echo'
t rebase rebase 'GIT_PS1_SHOWCONFLICTSTATE=yes; __git_ps1; echo'
t cherry cherry '__git_ps1; echo'
t revert revert '__git_ps1; echo'
t seq-pick seq-pick '__git_ps1; echo'
t seq-revert seq-revert '__git_ps1; echo'
t seq-p seq-p '__git_ps1; echo'
t seq-crlf seq-crlf '__git_ps1; echo'
t bisect bisect '__git_ps1; echo'
t am-applying am-applying '__git_ps1; echo'
t am-rebasing am-rebasing '__git_ps1; echo'
t am-neither am-neither '__git_ps1; echo'
# Git Bash keeps the CR of a CRLF line that `IFS=$'\r\n' read` reads; cash ends the line
# there, which is what __git_eread's IFS is asking for.
t crlf-rebase crlf-rebase '__git_ps1; echo' ' (crlf|REBASE 1/3)' D20
t eread-crlf crlf-rebase '__git_eread .git/rebase-merge/head-name v; echo "$? <$v>" ${#v}; __git_eread .git/nope v2; echo "$? <${v2-unset}>"' \
    $'0 <refs/heads/crlf> 15\n1 <unset>' D20

# --- special repositories and directories
t gitdir clean 'cd .git && __git_ps1; echo'
t gitdir-objects clean 'cd .git/objects && __git_ps1; echo'
t bare bare.git '__git_ps1; echo'
t sparse sparse '__git_ps1; echo'
t sparse-compress sparse 'GIT_PS1_COMPRESSSPARSESTATE=1; __git_ps1; echo'
t sparse-omit sparse 'GIT_PS1_OMITSPARSESTATE=1; __git_ps1; echo'
t hide-ignored ignored 'cd build && GIT_PS1_HIDE_IF_PWD_IGNORED=1 && __git_ps1; echo "<$?>"'
t hide-notignored ignored 'GIT_PS1_HIDE_IF_PWD_IGNORED=1; __git_ps1; echo'
t hide-unset ignored 'cd build && __git_ps1; echo'
t cfg-nohide cfg-nohide 'cd build && GIT_PS1_HIDE_IF_PWD_IGNORED=1 && __git_ps1; echo'
t worktree wt 'GIT_PS1_SHOWDIRTYSTATE=1; __git_ps1; echo'
t reftable reftable 'GIT_PS1_SHOWDIRTYSTATE=1; __git_ps1; echo'
t evil evil '__git_ps1; echo'
t percent percent '__git_ps1; echo; __git_ps1 "<%s>"; echo'

# --- colors, command-substitution mode
t color-clean clean 'GIT_PS1_SHOWCOLORHINTS=1; __git_ps1'
t color-allflags allflags 'GIT_PS1_SHOWCOLORHINTS=1 GIT_PS1_SHOWDIRTYSTATE=1 GIT_PS1_SHOWSTASHSTATE=1 GIT_PS1_SHOWUNTRACKEDFILES=1; __git_ps1'
t color-detached detached-notag 'GIT_PS1_SHOWCOLORHINTS=1; __git_ps1'
t color-bare bare.git 'GIT_PS1_SHOWCOLORHINTS=1; __git_ps1'
t color-markers allflags 'GIT_PS1_SHOWCOLORHINTS=1 GIT_PS1_SHOWDIRTYSTATE=1 GIT_PS1_COLOR_PRE="\[" GIT_PS1_COLOR_POST="\]"; __git_ps1'
t color-nomarkers allflags 'GIT_PS1_SHOWCOLORHINTS=1 GIT_PS1_SHOWDIRTYSTATE=1 GIT_PS1_COLOR_PRE= GIT_PS1_COLOR_POST=; __git_ps1'

# --- PROMPT_COMMAND mode (2 or 3 arguments), which sets PS1
t pc-2 clean '__git_ps1 "\u@\h:\w" "\\\$ "; printf "%s\n" "$PS1" "$__git_ps1_branch_name"'
t pc-3 allflags 'GIT_PS1_SHOWDIRTYSTATE=1; __git_ps1 "A" "B" "{%s}"; printf "%s\n" "$PS1"'
t pc-norepo norepo '__git_ps1 "pre " "post"; printf "%s\n" "$PS1"'
t pc-nopromptvars clean 'shopt -u promptvars; __git_ps1 "A" "B"; printf "%s\n" "$PS1"'
t pc-color allflags 'GIT_PS1_SHOWCOLORHINTS=1 GIT_PS1_SHOWDIRTYSTATE=1 GIT_PS1_SHOWUNTRACKEDFILES=1; __git_ps1 "\w" "\$ "; printf "%s" "$PS1"'
t pc-upstream-name up-diverged 'GIT_PS1_SHOWUPSTREAM="verbose name"; __git_ps1 "A" "B"; printf "%s\n" "$PS1" "$__git_ps1_upstream_name"'
t pc-evil evil '__git_ps1 "A" "B"; printf "%s\n" "$PS1" "$__git_ps1_branch_name"'
t pc-exit clean '(exit 5); __git_ps1 "A" "B"; echo "<$?>"'

# --- the prompt as it would be drawn: ${PS1@P}
t P-cmdsub clean 'PS1="[\W\$(__git_ps1 \" (%s)\")]\\\$ "; printf "%s\n" "${PS1@P}"'
t P-backtick clean 'PS1="[\W\`__git_ps1\`]\\\$ "; printf "%s\n" "${PS1@P}"'
t P-cmdsub-w clean 'cd sub; PS1="\w\$(__git_ps1)\\\$ "; printf "%s\n" "${PS1@P}"'
t P-pc-2 clean '__git_ps1 "\W" "\\\$ "; printf "%s\n" "${PS1@P}"'
t P-pc-color allflags 'GIT_PS1_SHOWCOLORHINTS=1 GIT_PS1_SHOWDIRTYSTATE=1; __git_ps1 "\W" "\\\$ "; printf "%s" "${PS1@P}"'
t P-pc-evil evil '__git_ps1 "\W" "\\\$ "; printf "%s\n" "${PS1@P}"'
t P-pc-percent percent '__git_ps1 "\W" "\\\$ "; printf "%s\n" "${PS1@P}"'
t P-pc-upstream up-diverged 'GIT_PS1_SHOWUPSTREAM="verbose name"; __git_ps1 "\W" "\\\$ "; printf "%s\n" "${PS1@P}"'
t P-cmdsub-color allflags 'GIT_PS1_SHOWCOLORHINTS=1 GIT_PS1_SHOWDIRTYSTATE=1; PS1="\W\$(__git_ps1)\\\$ "; printf "%s" "${PS1@P}"'

# --- Git for Windows' profile.d/git-prompt.sh, which sources git-completion.bash too.
# Its PS1 shows $PWD (C:/ in cash, /c/ in Git Bash) and \h (%COMPUTERNAME% in cash, the
# DNS host name in Git Bash); both are masked before expanding it.
if [[ -f $PD ]]; then
    t pd-ps1 clean "MSYSTEM=MINGW64; . '$PD'; printf '%s\n' \"\$PS1\" \"\$TITLEPREFIX\"; shopt -p no_empty_cmd_completion"
    t pd-expand dirty-w "MSYSTEM=MINGW64; . '$PD'; PS1=\${PS1//\\\$PWD/PWD}; PS1=\${PS1//\\\\h/HOST}; printf '%s' \"\${PS1@P}\""
    t pd-complete-spec clean "MSYSTEM=MINGW64; . '$PD'; complete -p git gitk"
    t pd-userconfig pd-config "mkdir -p .config/git; echo 'PS1=custom' >.config/git/git-prompt.sh; HOME=\$PWD; MSYSTEM=MINGW64; . '$PD'; echo \"\$PS1\"; rm -r .config"

    # git-completion.bash, driven the way readline would drive it.
    C='_c() { COMP_LINE=$1; COMP_POINT=${#1}; read -ra COMP_WORDS <<<"$1"; [[ $1 == *" " ]] && COMP_WORDS+=(""); COMP_CWORD=$((${#COMP_WORDS[@]} - 1)); COMPREPLY=(); __git_wrap__git_main; printf "<%s>" "${COMPREPLY[@]}"; echo; }'
    t comp-subcmd clean "MSYSTEM=MINGW64; . '$PD'; $C; _c 'git che'; _c 'git sta'"
    t comp-branch up-diverged "MSYSTEM=MINGW64; . '$PD'; $C; _c 'git checkout '; _c 'git switch m'"
    t comp-option clean "MSYSTEM=MINGW64; . '$PD'; $C; _c 'git commit --am'; _c 'git log --onel'"
    t comp-file dirty-w "MSYSTEM=MINGW64; . '$PD'; $C; _c 'git add '; _c 'git restore s'"
    t comp-config clean "MSYSTEM=MINGW64; . '$PD'; $C; _c 'git config core.autoc'; _c 'git -c color.u'"
    t comp-remote up-diverged "MSYSTEM=MINGW64; . '$PD'; $C; _c 'git push '; _c 'git push origin m'"
fi

# Eight at a time; each case's lines are printed in order once all are done.
mkdir -p "$W/out" "$W/results"
if [[ $mode == freeze ]]; then
    mkdir -p "$G"
    rm -f "$G"/*.out "$G"/*.status
    sources >"$G/SOURCES"
fi
for name in "${order[@]}"; do
    while (($(jobs -rp | wc -l) >= 8)); do wait -n; done
    run "$name" >"$W/results/$name" 2>&1 &
done
wait
for name in "${order[@]}"; do cat "$W/results/$name"; done >"$W/all"
if [[ $mode == freeze ]]; then
    printf 'froze %d case(s) into %s\n' "${#order[@]}" "$G"
    exit 0
fi
if [[ $mode == check ]]; then
    for golden in "$G"/*.out; do
        name=${golden##*/} name=${name%.out}
        [[ -f $W/cases/$name.sh ]] || printf 'FAIL %s\n  frozen, but no longer a case: re-freeze\n' "$name" >>"$W/all"
    done
fi
cat "$W/all"
passes=$(grep -c '^PASS' "$W/all") failures=$(grep -c '^FAIL' "$W/all")
printf '%d passed, %d failed\n' "$passes" "$failures"
((failures == 0 && passes == ${#order[@]}))
