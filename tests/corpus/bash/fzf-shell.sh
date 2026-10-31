# source: https://github.com/junegunn/fzf/tree/v0.74.5 (`shell/completion.bash`, `shell/key-bindings.bash` and LICENSE, unchanged, as the fixture directory `fzf`)
# desc: fzf's Bash integration with a stand-in fzf that answers the first line holding the query: both scripts want an interactive shell, so a second shell of the same kind runs them; the completions set up for the path, directory and variable commands and the ones they replace, the `**` trigger completing a file, a directory and a variable through the compgen hooks, process substitution and compgen, aliases, a completion added with _fzf_setup_completion, the defaults line, Ctrl-T's file list and the line it edits, and Alt-C's directory
mkdir -p bin src/lib docs && touch src/main.c src/lib/util.c docs/guide.md README
printf '%s\n' '#!/bin/sh' \
  '# A stand-in for fzf: the first input line holding the query, or the first line; the' \
  '# input is FZF_DEFAULT_COMMAND when set, as fzf runs it at a terminal, else stdin.' \
  'query=; read0=' \
  'while [ $# -gt 0 ]; do case $1 in -q|--query) query=$2; shift;; --query=*) query=${1#*=};; --read0) read0=1;; esac; shift; done' \
  'if [ -n "$FZF_DEFAULT_COMMAND" ]; then sh -c "$FZF_DEFAULT_COMMAND"; elif [ -n "$read0" ]; then tr "\0" "\n"; else cat; fi |' \
  '  { if [ -n "$query" ]; then grep -F -m1 -- "$query"; else head -n1; fi; }' > bin/fzf
chmod +x bin/fzf
export PATH="$PWD/bin:$PATH" HOME="$PWD/home"
mkdir -p "$HOME"
CASH_NO_OFFER=1 "$BASH" --noprofile --norc -i -c '
_fzf_compgen_path() { command find -L "$1" -mindepth 1 \( -path "*/.git" \) -prune -o -type f -print -o -type d -print 2> /dev/null | sort | sed "s@^\./@@"; }
_fzf_compgen_dir() { command find -L "$1" -mindepth 1 \( -path "*/.git" \) -prune -o -type d -print 2> /dev/null | sort | sed "s@^\./@@"; }
. fzf/shell/completion.bash
. fzf/shell/key-bindings.bash
echo "== completions set up"; complete -p cd vim export unalias | sort
echo "== defaults"; __fzf_defaults "--reverse --scheme=path" "--multi"
words() { COMP_WORDS=("$@"); COMP_CWORD=$(($# - 1)); COMP_LINE="$*"; COMP_POINT=${#COMP_LINE}; COMPREPLY=(); }
reply() { tr -d "\033" < completion.out; printf "\nreply: %s\n" "${COMPREPLY[*]}"; }
echo "== a file for vim"; words vim "src/**"; _fzf_path_completion vim "src/**" vim > completion.out; reply
echo "== a file by a query"; words cat "util**"; _fzf_path_completion cat "util**" cat > completion.out; reply
echo "== a directory for cd"; words cd "**"; _fzf_dir_completion cd "**" cd > completion.out; reply
echo "== no trigger, the original completion"; words cd "sr"; _fzf_dir_completion cd "sr" cd; printf "reply: %s\n" "${COMPREPLY[*]}"
export FZF_TEST_ONE=1 FZF_TEST_TWO=2
echo "== a variable for export"; words export "FZF_TEST**"; _fzf_var_completion export "FZF_TEST**" export > completion.out; reply
alias fzf_alias_a="echo a" fzf_alias_b="echo b"
echo "== an alias for unalias"; words unalias "fzf_alias**"; _fzf_alias_completion unalias "fzf_alias**" unalias > completion.out; reply
echo "== a completion added"; _fzf_setup_completion path ag rg; complete -p ag rg | sort; _fzf_setup_completion; echo "usage: $?"
echo "== Ctrl-T"; FZF_CTRL_T_COMMAND="find . -type f | sort"; __fzf_select__
echo; READLINE_LINE="vim " READLINE_POINT=4; fzf-file-widget; printf "%s|%s\n" "$READLINE_LINE" "$READLINE_POINT"
echo "== Alt-C"; FZF_ALT_C_COMMAND="find . -type d -name lib | sort"; __fzf_cd__ | sed "s|$(builtin pwd)|PWD|"; echo
' 2> /dev/null
echo "inner status: $?"
