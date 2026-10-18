# source: https://github.com/dylanaraps/pure-bash-bible#get-the-name-of-the-current-function
# desc: FUNCNAME through three calls, $PWD against pwd, SECONDS assigned, RANDOM in range and seeded, and OSTYPE's case
# tags: cash-divergence (spec section 4 row 25: $OSTYPE is windows, which the case's cygwin | msys | win32 misses)
inner() { printf '%s\n' "${FUNCNAME[0]} ${FUNCNAME[1]} ${FUNCNAME[2]}"; printf '%s|' "${FUNCNAME[@]}"; printf '\n'; }
middle() { inner; }
outer() { middle; }
outer
printf '[%s]\n' "${FUNCNAME[0]}"
[[ $PWD == "$(pwd)" ]] && echo "PWD is pwd"
SECONDS=100
(( SECONDS >= 100 && SECONDS < 105 )) && echo "SECONDS counts from what it was given"
ok=1
for _ in {1..200}; do
    (( RANDOM >= 0 && RANDOM <= 32767 )) || ok=0
done
echo "RANDOM in range: $ok"
RANDOM=42
a="$RANDOM $RANDOM $RANDOM"
RANDOM=42
b="$RANDOM $RANDOM $RANDOM"
[[ $a == "$b" ]] && echo "a seed repeats the sequence"
case "$OSTYPE" in
    "darwin"*) : "MacOS" ;;
    "linux"*) : "Linux" ;;
    *"bsd"* | "dragonfly" | "bitrig") : "BSD" ;;
    "cygwin" | "msys" | "win32") : "Windows" ;;
    *) : "Unknown" ;;
esac
os="$_"
printf '%s\n' "$os"
