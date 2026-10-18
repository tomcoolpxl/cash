# source: https://github.com/dylanaraps/pure-sh-bible#check-if-a-number-is-an-integer
# desc: is_float and is_int by printf %f and %d accepting the argument, on many spellings of numbers
is_float() {
    # Usage: is_float "number"
    [ -z "${1##*.*}" ] &&
        printf %f "$1" >/dev/null 2>&1
}
is_int() {
    # usage: is_int "number"
    printf %d "$1" >/dev/null 2>&1
}
for n in 1 -2 +3 1.5 -0.25 .5 5. 1.2.3 1e3 1.5e3 0x1A 0X1a 017 08 abc '' ' 3' '3 ' 9223372036854775807 9223372036854775808 "'A" '"B'; do
    f=no; i=no
    is_float "$n" && f=yes
    is_int "$n" && i=yes
    printf '%-22s float=%s int=%s\n' "[$n]" "$f" "$i"
done
printf '%d %d %d %d\n' 0x1A 017 "'A" -0
printf '%.2f %.1f %f\n' 1.005 .25 1e3
printf '%d\n' 1.5 2>/dev/null; echo "status $?"
printf '%d\n' abc 2>/dev/null; echo "status $?"
