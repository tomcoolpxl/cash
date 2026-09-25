# source: https://github.com/dylanaraps/pure-bash-bible#progress-bars
# desc: printf -v with dynamic width and ${prog// /-}
bar() {
    # Usage: bar 1 10
    #            ^----- Elapsed Percentage (0-100).
    #               ^-- Total length in chars.
    ((elapsed=$1*$2/100))

    # Create the bar with spaces.
    printf -v prog  "%${elapsed}s"
    printf -v total "%$(($2-elapsed))s"

    printf '%s\r' "[${prog// /-}${total}]"
}
for i in 0 10 55 100; do bar "$i" 10; echo; done | tr '\r' R
