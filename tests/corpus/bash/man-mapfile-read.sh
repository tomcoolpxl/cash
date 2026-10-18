# source: https://www.gnu.org/software/bash/manual/bash.html#index-mapfile (and read; cash's own case)
# desc: mapfile -t -n -s -O -d -C -c, read -d -n -N -a -r -s and IFS, on here-strings and process substitutions
printf 'l1\nl2\nl3\nl4\nl5\n' >lines
mapfile -t all <lines; printf '[%s]' "${all[@]}"; echo " ${#all[@]}"
mapfile -t -n 2 -s 1 part <lines; printf '[%s]' "${part[@]}"; echo
mapfile -t -O 10 offset <lines; echo "${!offset[@]}"
mapfile -d , -t csv <<<'a,b,,c'; printf '[%s]' "${csv[@]}"; echo
report() { printf 'callback %s:%s' "$1" "$2"; }
mapfile -t -C report -c 2 cb <lines; echo; echo "${#cb[@]}"
mapfile keep < <(printf 'x\ny\n'); printf '{%s}' "${keep[@]}"; echo
read -r -d ';' first <<<'one;two'; echo "[$first]"
read -r -n 3 three <<<'abcdef'; echo "[$three]"
read -r -N 4 four <<<$'ab\ncd'; printf '[%s]\n' "$four"
read -r -a words <<<'  w1   w2 w3  '; printf '[%s]' "${words[@]}"; echo
IFS=: read -r f1 f2 rest <<<'a:b:c:d'; echo "[$f1][$f2][$rest]"
IFS=: read -r only <<<'a:b:'; echo "[$only]"
read -r back <<<'a\b'; echo "[$back]"
read nobs <<<'a\b \
continued'; echo "[$nobs]"
read -r x <<<''; echo "empty status $? [$x]"
printf 'no newline' | { read -r y; echo "status $? [$y]"; }
read -r -t 0 <<<'ready'; echo "t0 status $?"
