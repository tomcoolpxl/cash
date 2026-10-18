# source: https://github.com/dylanaraps/pure-bash-bible#bypass-shell-aliases
# desc: a backslash or quotes bypass an alias, command bypasses a function, and a background subshell's output is dropped
shopt -s expand_aliases
alias say='echo aliased'
say one
\say two 2>/dev/null || echo "no command named say"
'say' three 2>/dev/null || echo "quoted say is no alias either"
alias echo='echo aliased-echo'
echo four
\echo five
unalias echo
printf() { builtin printf 'function: %s\n' "$@"; }
printf six
command printf 'command: %s\n' seven
unset -f printf
bkr() {
    (nohup "$@" &>/dev/null &)
}
bkr echo "this goes nowhere"
echo end
