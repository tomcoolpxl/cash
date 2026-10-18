# source: https://github.com/dylanaraps/pure-bash-bible#file-conditionals
# desc: -a -d -e -f -r -s -w and -t, -ef, -nt and -ot, in [[ ]] and in [ ]
printf 'data' >full
: >empty
mkdir -p adir
touch -d '2001-01-01 00:00:00' old
touch -d '2011-01-01 00:00:00' new
for f in full empty adir missing; do
    printf '%s:' "$f"
    for op in -a -d -e -f -r -s -w; do
        if [[ $op $f ]] 2>/dev/null; then printf ' %s' "$op"; fi
    done
    printf '\n'
done
for f in full empty adir missing; do
    printf '%s:' "$f"
    [ -d "$f" ] && printf ' d'
    [ -e "$f" ] && printf ' e'
    [ -f "$f" ] && printf ' f'
    [ -s "$f" ] && printf ' s'
    printf '\n'
done
[[ -a full ]] && echo "unary -a in [[ ]] tests existence"
[[ -t 0 ]] || echo "stdin is not a terminal"
[[ -t 5 ]] || echo "fd 5 is not open"
[[ full -ef full ]] && echo "full -ef full"
[[ full -ef ./adir/../full ]] && echo "full -ef ./adir/../full"
[[ full -ef empty ]] || echo "full is not empty"
[[ new -nt old ]] && echo "new -nt old"
[[ old -ot new ]] && echo "old -ot new"
[[ new -nt missing ]] && echo "new -nt missing"
[[ missing -ot new ]] && echo "missing -ot new"
[[ missing -nt missing ]] || echo "missing is not newer than missing"
