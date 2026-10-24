# --- Prompt ---------------------------------------------------------------------------
#   ~/src/project (main)
#   ❯
# The branch is read from the repository's own files by the shell itself, with no `git`
# process and no need for Git to be installed, so the prompt costs nothing to draw; the
# arrow is red after a command that failed. Where the repository is, is looked for only
# when the directory changes.

# Sets __prompt_gitdir to the repository's git directory for $PWD, empty outside one;
# walked up only when $PWD changed since the last prompt. Handles worktrees and
# submodules, whose .git is a file naming the real one.
__prompt_find_git() {
    [[ $PWD == "$__prompt_git_pwd" ]] && return
    __prompt_git_pwd=$PWD
    __prompt_gitdir=
    local dir=$PWD git=
    while :; do
        if [[ -f $dir/.git/HEAD ]]; then
            git=$dir/.git
        elif [[ -f $dir/.git ]]; then
            read -r git < "$dir/.git"
            git=${git#gitdir: }
            [[ $git == /* || $git == [A-Za-z]:* ]] || git=$dir/$git
        fi
        if [[ -n $git ]]; then
            __prompt_gitdir=$git
            return
        fi
        [[ $dir == */* ]] || return
        dir=${dir%/*}
    done
}

# Sets __prompt_branch to the branch, the tag or short commit when detached, and what Git
# is in the middle of, as Git's own __git_ps1 shows it: `main|MERGING`, `main|REBASE 2/5`.
# Empty outside a repository.
__prompt_git_branch() {
    __prompt_branch=
    __prompt_find_git
    local g=$__prompt_gitdir head state= step= total=
    [[ -n $g && -f $g/HEAD ]] && read -r head < "$g/HEAD" || return
    if [[ -d $g/rebase-merge ]]; then
        read -r head < "$g/rebase-merge/head-name" 2> /dev/null && head="ref: $head"
        read -r step < "$g/rebase-merge/msgnum" 2> /dev/null
        read -r total < "$g/rebase-merge/end" 2> /dev/null
        state=REBASE
    elif [[ -d $g/rebase-apply ]]; then
        read -r step < "$g/rebase-apply/next" 2> /dev/null
        read -r total < "$g/rebase-apply/last" 2> /dev/null
        if [[ -f $g/rebase-apply/rebasing ]]; then
            read -r head < "$g/rebase-apply/head-name" 2> /dev/null && head="ref: $head"
            state=REBASE
        elif [[ -f $g/rebase-apply/applying ]]; then
            state=AM
        else
            state=AM/REBASE
        fi
    elif [[ -f $g/MERGE_HEAD ]]; then
        state=MERGING
    elif [[ -f $g/CHERRY_PICK_HEAD ]]; then
        state=CHERRY-PICKING
    elif [[ -f $g/REVERT_HEAD ]]; then
        state=REVERTING
    elif [[ -f $g/BISECT_LOG ]]; then
        state=BISECTING
    fi
    if [[ $head == 'ref: refs/heads/'* ]]; then
        __prompt_branch=${head#ref: refs/heads/}
    else
        __prompt_branch=$(__prompt_tag "$g" "$head") || __prompt_branch=${head:0:7}
    fi
    [[ -n $state ]] && __prompt_branch+="|$state"
    [[ -n $step && -n $total ]] && __prompt_branch+=" $step/$total"
}

# Prints the tag that names commit $2 in git directory $1, read from refs/tags and
# packed-refs; fails when none does. Only a detached HEAD asks.
__prompt_tag() {
    local tag sha name
    for tag in "$1"/refs/tags/*; do
        [[ -f $tag ]] && read -r sha < "$tag" && [[ $sha == "$2" ]] &&
            { printf '%s' "${tag##*/}"; return 0; }
    done
    [[ -f $1/packed-refs ]] || return 1
    while read -r sha name; do
        [[ $sha == "$2" && $name == refs/tags/* ]] &&
            { printf '%s' "${name#refs/tags/}"; return 0; }
    done < "$1/packed-refs"
    return 1
}

# The current directory as Windows spells it, for Windows Terminal: C:\Users\me, from
# cash's C:/Users/me or Git Bash's /c/Users/me. Empty for Git Bash's own /usr and such.
__prompt_windows_path() {
    local path=$PWD
    if [[ $path =~ ^/([A-Za-z])(/.*)?$ ]]; then
        path="${BASH_REMATCH[1]^}:${BASH_REMATCH[2]:-/}"
    elif [[ $path != [A-Za-z]:* && $path != //* ]]; then
        __prompt_winpath=
        return
    fi
    __prompt_winpath=${path//\//\\}
}

# Sets PS1 for the status $1 of the command before.
__prompt_draw() {
    local status=$1

    __prompt_git_branch
    local branch=
    [[ -n $__prompt_branch ]] && branch=" \[\e[35m\]($__prompt_branch)\[\e[0m\]"

    local arrow='\[\e[32m\]❯\[\e[0m\] '
    ((status == 0)) || arrow='\[\e[31m\]❯\[\e[0m\] '

    # As administrator (cash's EUID is 0 then): Unix's sign of root, a red #, and the
    # tab title says so.
    local title='\w'
    if ((EUID == 0)); then
        arrow='\[\e[1;31m\]#\[\e[0m\] '
        title='admin: \w'
    fi

    local body="\[\e[1;34m\]\w\[\e[0m\]$branch\n$arrow"

    if [[ -n $WT_SESSION ]]; then
        # Windows Terminal: the tab title, the directory for new tabs and panes (OSC 9;9),
        # and the marks around prompt, command and output (OSC 133), as Microsoft's
        # shell-integration guide sets them.
        __prompt_windows_path
        local cwd=
        # Each backslash doubled: in PS1 a lone one starts an escape (\t is the time).
        [[ -n $__prompt_winpath ]] && cwd="\[\e]9;9;${__prompt_winpath//\\/\\\\}\e\\\\\]"
        PS1="\[\e]133;D;$status\e\\\\\]\[\e]133;A\e\\\\\]\[\e]2;$title\a\]$cwd$body\[\e]133;B\e\\\\\]"
    else
        PS1="\[\e]2;$title\a\]$body"
    fi
}

[[ -n $WT_SESSION ]] && PS0='\[\e]133;C\e\\\]'
PS2='> '
