# source: https://www.gnu.org/software/bash/manual/bash.html#The-Shopt-Builtin (glob options; cash's own case)
# desc: nullglob, failglob, dotglob, nocaseglob, extglob patterns, globstar, GLOBIGNORE and character classes, in a directory of their own
mkdir -p g/sub/deep && cd g || exit
: >a.txt; : >B.txt; : >.hidden; : >c.log; : >sub/d.txt; : >sub/deep/e.txt; : >'with space.txt'
echo *
echo *.none
shopt -s nullglob; echo "nullglob: [$(echo *.none)]"; shopt -u nullglob
shopt -s failglob; ( echo *.none ) 2>/dev/null; echo "failglob status $?"; shopt -u failglob
shopt -s dotglob; echo *; shopt -u dotglob
echo .*
shopt -s nocaseglob; echo b*; shopt -u nocaseglob
echo [[:upper:]]* [[:lower:]].*
shopt -s extglob
echo !(*.txt)
echo +(a|c).*
echo @(a|B).txt
echo *.@(log|none)
echo ?(x)a.txt
shopt -s globstar
echo **/*.txt
echo sub/**
shopt -u globstar
GLOBIGNORE='*.log:B*'
echo *
unset GLOBIGNORE
echo *' '*
printf '[%s]\n' *space*
