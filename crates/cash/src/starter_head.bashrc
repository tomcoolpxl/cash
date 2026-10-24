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

