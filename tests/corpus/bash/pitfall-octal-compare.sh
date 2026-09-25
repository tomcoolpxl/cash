# source: https://mywiki.wooledge.org/BashPitfalls#pf7 ([[ $foo > 7 ]])
# desc: [ 010 -gt 8 ] true, [[ 010 -gt 8 ]] and (( 010 > 8 )) false
[ 010 -gt 8 ]; echo "[ ]: $?"
[[ 010 -gt 8 ]]; echo "[[ ]]: $?"
(( 010 > 8 )); echo "(( )): $?"
foo=9
[ "$((foo > 7))" -ne 0 ]; echo "posix: $?"
