# source: https://github.com/rcaloras/bash-preexec/tree/0.7.0 (`bash-preexec.sh`, unchanged, with its LICENSE.md, as the fixture directory `bash-preexec`)
# desc: the preexec and precmd hooks behind atuin and starship, driven from a script: its helpers trimming and sanitising, HISTCONTROL adjusted, the PS0 hook Bash 5.3 gets, then its DEBUG-trap hook for older Bash: an earlier DEBUG trap and PROMPT_COMMAND kept as hooks, functions given the trace attribute, a prompt run by hand through PROMPT_COMMAND, precmd hooks given the status before them, and preexec hooks given each command read with history on
# timeout: 30
source bash-preexec/bash-preexec.sh
echo "imported: ${bash_preexec_imported:-no} ${__bp_imported:-}"
__bp_trim_whitespace x '  a  b  '; printf '[%s]\n' "$x"
__bp_trim_whitespace y $'\t lead and tail \t'; printf '[%s]\n' "$y"
__bp_sanitize_string z ' ; echo one; : ; echo two;; '; printf '[%s]\n' "$z"
HISTCONTROL=ignorespace:erasedups; __bp_adjust_histcontrol; echo "HISTCONTROL=$HISTCONTROL"
HISTCONTROL=ignoreboth; __bp_adjust_histcontrol; echo "HISTCONTROL=$HISTCONTROL"
echo "hook: $__bp_hook_preexec_proc"
(__bp_hook_preexec_into_ps0; printf 'PS0=%s\n' "$PS0")
__bp_hook_preexec_proc=__bp_hook_preexec_into_debug
declare -F -p __bp_install __bp_hook_preexec_into_debug 2>/dev/null | sed 's/^declare //'
trap 'echo "old trap: $BASH_COMMAND" >/dev/null' DEBUG
PROMPT_COMMAND='echo old-prompt-command'
precmd() { echo "precmd: status $?"; }
preexec() { echo "preexec: [$1]"; }
precmd_functions+=(second_precmd)
second_precmd() { echo "second precmd"; }
__bp_install
trap -p DEBUG
declare -p PROMPT_COMMAND
printf '%s\n' "${preexec_functions[@]}" "--" "${precmd_functions[@]}"
set -o history
HISTFILE=/dev/null
prompt() { local c; for c in "${PROMPT_COMMAND[@]}"; do eval "$c"; done; }
false
prompt
echo first command
prompt
echo second; echo same line
prompt
echo "done"
