# ~/.bashrc: read by cash and by Git Bash. `cash --init-rc` wrote it as a starting point,
# and it is yours to change. Sections for cash alone check $CASH_VERSION, which Git Bash
# does not set.

# Only interactive shells: a script, `bash -c` or `cash -c` gets none of this, and no
# failed status from it either.
[[ $- == *i* ]] || return 0

# --- History --------------------------------------------------------------------------
# Big, with no repeats and nothing typed after a leading space; appended to the file
# after every command, so a new window sees what the others ran.
HISTSIZE=50000
HISTFILESIZE=50000
HISTCONTROL=ignoreboth:erasedups
HISTTIMEFORMAT='%F %T  '
shopt -s histappend cmdhist

# --- Shell options --------------------------------------------------------------------
shopt -s checkwinsize globstar

# --- Aliases --------------------------------------------------------------------------
alias ll='ls -alFh'
alias la='ls -A'
alias l='ls -CF'
alias ..='cd ..'
alias ...='cd ../..'
alias ....='cd ../../..'
alias grep='grep --color=auto'
alias diff='diff --color=auto'
alias mkdir='mkdir -p'
alias cp='cp -i'
alias mv='mv -i'
alias path='echo "${PATH//:/$'"'"'\n'"'"'}"'

# --- Prompt ---------------------------------------------------------------------------
#   ~/src/project (main)
#   ❯
# The branch is read from .git/HEAD by the shell itself, with no `git` process, so the
# prompt costs nothing to draw; the arrow is red after a command that failed.

# Sets __prompt_branch to the branch, or the short commit when detached; empty outside
# a repository. Handles worktrees and submodules, whose .git is a file naming the real one.
__prompt_git_branch() {
    __prompt_branch=
    local dir=$PWD git head
    while :; do
        if [[ -f $dir/.git/HEAD ]]; then
            git=$dir/.git
        elif [[ -f $dir/.git ]]; then
            read -r git < "$dir/.git"
            git=${git#gitdir: }
            [[ $git == /* || $git == [A-Za-z]:* ]] || git=$dir/$git
        fi
        if [[ -n $git ]]; then
            [[ -f $git/HEAD ]] && read -r head < "$git/HEAD" || return
            if [[ $head == 'ref: refs/heads/'* ]]; then
                __prompt_branch=${head#ref: refs/heads/}
            else
                __prompt_branch=${head:0:7}
            fi
            return
        fi
        [[ $dir == */* ]] || return
        dir=${dir%/*}
    done
}

# The current folder as Windows spells it, for Windows Terminal: C:\Users\me, from
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

__prompt_command() {
    local status=$?
    history -a

    __prompt_git_branch
    local branch=
    [[ -n $__prompt_branch ]] && branch=" \[\e[35m\]($__prompt_branch)\[\e[0m\]"

    local arrow='\[\e[32m\]❯\[\e[0m\] '
    ((status == 0)) || arrow='\[\e[31m\]❯\[\e[0m\] '

    local body="\[\e[1;34m\]\w\[\e[0m\]$branch\n$arrow"

    if [[ -n $WT_SESSION ]]; then
        # Windows Terminal: the tab title, the folder for new tabs and panes (OSC 9;9),
        # and the marks around prompt, command and output (OSC 133), as Microsoft's
        # shell-integration guide sets them.
        __prompt_windows_path
        local cwd=
        # Each backslash doubled: in PS1 a lone one starts an escape (\t is the time).
        [[ -n $__prompt_winpath ]] && cwd="\[\e]9;9;${__prompt_winpath//\\/\\\\}\e\\\\\]"
        PS1="\[\e]133;D;$status\e\\\\\]\[\e]133;A\e\\\\\]\[\e]2;\w\a\]$cwd$body\[\e]133;B\e\\\\\]"
    else
        PS1="\[\e]2;\w\a\]$body"
    fi
}

PROMPT_COMMAND=__prompt_command
[[ -n $WT_SESSION ]] && PS0='\[\e]133;C\e\\\]'
PS2='> '

# --- cash only ------------------------------------------------------------------------
if [[ -n $CASH_VERSION ]]; then
    # Icons before names, folders first; `ls` in a pipe or a script stays plain. The
    # icons need a Nerd Font: `scoop install nerd-fonts/CascadiaMono-NF`, then
    # `cash --terminal-profile` to have Windows Terminal use it.
    alias ls='ls --icons --group-directories-first'
else
    # Git Bash: its own ls alias, with folders first too.
    alias ls='ls -F --color=auto --show-control-chars --group-directories-first'
fi

if [[ -n $CASH_VERSION ]]; then
    # The banner, once per window: not again in a shell started from this one.
    if ((${SHLVL:-1} <= 1)); then
        coolfetch
    fi
fi
