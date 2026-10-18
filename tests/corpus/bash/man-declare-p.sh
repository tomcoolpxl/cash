# source: https://www.gnu.org/software/bash/manual/bash.html#index-declare (declare -p; cash's own case)
# desc: declare -p for every attribute and kind, quoting of odd values, functions with -f and -F, and local scopes
a=plain
b='with space'
c=$'new\nline'
d="it's"
e='$dollar `tick` "quote"'
declare -i i=7
declare -l lo=MIXED
declare -u up=mixed
declare -r ro=constant
declare -x ex=value
declare -a arr=(zero '' 'two words')
declare -A one=([key]=value)
declare -n nref=a
declare -t traced=t
declare -ix both=3
declare unset_but_declared
for v in a b c d e i lo up ro ex arr one nref traced both unset_but_declared; do
    declare -p "$v"
done
declare -p no_such_variable 2>/dev/null; echo "status $?"
sparse=([3]=c [1]=a)
declare -p sparse
f() { local x=1; local -i y=2; local -a z=(q); declare -p x y z; }
f
g() { echo "$1"; }
declare -f g
declare -F g f
declare -pf g | head -1
declare +x ex; declare -p ex
declare -p i lo | head -1
