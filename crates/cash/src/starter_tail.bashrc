# The starter's own: history appended after every command, so a new window sees what the
# others ran.
__prompt_command() {
    local status=$?
    history -a
    __prompt_draw "$status"
}
PROMPT_COMMAND=__prompt_command

# --- cash only ------------------------------------------------------------------------
if [[ -n $CASH_VERSION ]]; then
    # Icons before names, directories first; `ls` in a pipe or a script stays plain. The
    # icons need a Nerd Font: `scoop install nerd-fonts/CascadiaMono-NF`, then
    # `cash --terminal-profile` to have Windows Terminal use it.
    alias ls='ls --icons --group-directories-first'
else
    # Git Bash: its own ls alias, with directories first too.
    alias ls='ls -F --color=auto --show-control-chars --group-directories-first'
fi

if [[ -n $CASH_VERSION ]]; then
    # The banner, and where help is, once per window: not again in a shell started from
    # this one.
    if ((${SHLVL:-1} <= 1)); then
        coolfetch
        if [[ -t 1 && -z ${NO_COLOR-} ]]; then
            printf 'cash: press \e[36mF1\e[0m or type \e[36mman\e[0m for help\n'
        else
            echo 'cash: press F1 or type "man" for help'
        fi
    fi
fi
