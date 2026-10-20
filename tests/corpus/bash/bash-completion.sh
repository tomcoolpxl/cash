# source: https://github.com/scop/bash-completion/tree/2.18.0 (`bash_completion`, unchanged, with its COPYING, as the fixture directory `bash-completion`)
# desc: the bash-completion library, a 3,000-line file of completion helpers, driven from a script: quoting and dequoting, splitting into arrays, its compgen wrapper with filters and prefixes, the command-line words split as readline gives them, file and directory names completed with spaces and globs, long options read from a command's --help, arguments counted, and the completions it registers; the completions directory, which it loads on demand, is not there
# timeout: 60
shopt -s extglob progcomp
. ./bash-completion/bash_completion
echo "loaded: ${BASH_COMPLETION_VERSINFO[*]}"
show() { printf '%s\n' "${COMPREPLY[@]}" | LC_ALL=C sort; echo "--"; }
_comp_quote "a b'c"; printf 'quote: %s\n' "$REPLY"
_comp_dequote "'a b'\"c d\"e\\ f"; printf 'dequote: [%s]\n' "$REPLY"
_comp_split words 'x y  z'; declare -p words
_comp_split -F , fields '1,2,,3'; declare -p fields
COMPREPLY=()
cur=ga
_comp_compgen -- -W 'alpha beta gamma garnet'; show
_comp_compgen -c b -- -W 'alpha beta gamma'; show
_comp_compgen -R -- -W 'x y'; show
mkdir work && cd work || exit 1
mkdir -p dir1/sub dir2 && : > file1.txt && : > file2.log && : > 'with space.txt'
cur=fi; COMPREPLY=(); _comp_compgen_filedir; show
cur=w; COMPREPLY=(); _comp_compgen_filedir; show
cur=''; COMPREPLY=(); _comp_compgen_filedir -d; show
cur=''; COMPREPLY=(); _comp_compgen_filedir 'txt'; show
printf '%s\n' '#!/bin/sh' 'cat <<EOF' 'Usage: tool [options]' '  -a, --all          everything' '      --color[=WHEN] colour' '  -o, --output=FILE  write it' '      --verbose' 'EOF' > tool && chmod +x tool
COMP_WORDS=(./tool --); COMP_CWORD=1; COMP_LINE='./tool --'; COMP_POINT=9
COMPREPLY=(); _comp_complete_longopt ./tool -- ./tool; show
COMP_WORDS=(cmd -x one two); COMP_CWORD=3; COMP_LINE='cmd -x one two'; COMP_POINT=14
_comp_initialize -- cmd two one && echo "cur=$cur prev=$prev cword=$cword words=${words[*]}"
_comp_count_args; echo "args: $REPLY"
COMP_WORDS=(cmd 'a=b' c:d); COMP_CWORD=2; COMP_LINE='cmd a=b c:d'; COMP_POINT=11
_comp_get_words -n := cur prev; echo "words with = and : kept: cur=$cur prev=$prev"
_comp_looks_like_path ./x && echo "./x looks like a path"
_comp_looks_like_path plain || echo "plain does not"
complete -p | wc -l | sed 's/^ *//;s/[0-9]\+/some/'
complete -p ls 2>&1 | head -1
