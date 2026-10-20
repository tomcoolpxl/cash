# source: https://github.com/rupa/z/blob/v1.12/z.sh (the fixture `z.sh`, unchanged)
# desc: a real script sourced: its jump list kept in a data file through awk, ranks raised on each visit, -r -l and -e, two words matched in order, -c below the current directory, a directory gone from the list, the data file read back; under cash's unixpaths, with -x, which deletes with sed "\:^$PWD|.*:d", where $PWD has no colon
# unixpaths: $PWD is /c/..., as in Git Bash, so this runs without the C:/ workaround of
# the case it twins; Bash has no such option, and says so on standard error.
shopt -s unixpaths 2>/dev/null
export _Z_DATA="$PWD/zdata" _Z_NO_PROMPT_COMMAND=1
. ./z.sh
root=$PWD
mkdir -p proj/alpha proj/beta docs/alpha docs/gone
for i in 1 2 3; do _z --add "$root/proj/alpha"; done
_z --add "$root/proj/beta"
for i in 1 2; do _z --add "$root/docs/alpha"; done
_z --add "$root/docs/gone"
rmdir docs/gone
_z -r -l alpha 2>&1 | sed "s|$root/||"
_z -r -e alpha | sed "s|$root/||"
_z -r -e docs alpha | sed "s|$root/||"
_z -r -e nosuch; echo "no match: $?"
_z -r beta; echo "jumped to ${PWD#"$root"/}"
cd "$root/docs" && _z -r -c -e alpha | sed "s|$root/||"
cd "$root/proj/beta" && _z -x
cd "$root"
_z -r -l 2>&1 | sed "s|$root/||"
cut -d'|' -f1,2 zdata | sed "s|$root/||" | sort
