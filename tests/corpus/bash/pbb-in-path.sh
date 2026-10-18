# source: https://github.com/dylanaraps/pure-bash-bible#check-if-a-program-is-in-the-users-path
# desc: type -p, hash and command -v for a missing program, a builtin, a function and an alias
type -p no_such_program_x &>/dev/null; echo "type -p missing: $?"
hash no_such_program_x &>/dev/null; echo "hash missing: $?"
command -v no_such_program_x &>/dev/null; echo "command -v missing: $?"
printf '[%s] %s\n' "$(type -p printf)" "$?"
printf '[%s] %s\n' "$(command -v printf)" "$?"
f() { :; }
printf '[%s] %s\n' "$(type -p f)" "$?"
printf '[%s] %s\n' "$(command -v f)" "$?"
shopt -s expand_aliases
alias ll='ls -l'
printf '[%s] %s\n' "$(command -v ll)" "$?"
printf '[%s]\n' "$(type -t printf) $(type -t f) $(type -t ll) $(type -t if) $(type -t no_such_program_x)"
if ! type -p no_such_program_x &>/dev/null; then
    printf '%s\n' "error: no_such_program_x is not installed"
fi
