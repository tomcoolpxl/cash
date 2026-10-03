# Infinite and NaN numbers, written and read as gawk writes and reads them.
BEGIN {
    inf = -log(0)
    nan = -log(-1)
    print inf, -inf, nan, -nan
    printf "%d|%i|%u|%o|%x|%X|%f|%F|%e|%E|%g|%G|%a|%A\n", inf, -inf, inf, inf, inf, -inf, inf, -inf, nan, -nan, inf, -inf, inf, -nan
    printf "%05d|%-6f|%+e|% g|%.1f|%6.3s|%-7d|%7X|\n", inf, inf, nan, inf, -inf, inf, -inf, inf
    s = inf ""
    print s, length(inf), int(inf), substr(-inf "", 2)
    n = split("inf|nan|+infinity|+nan|-inf|0x1A|1e500|-1e500|+INF|infx|+inf |  -nan|+infx|+nan5|-i|.5e|+.5|-0|1e|1e+|1e+3x|5.|.|-|\t7\v", a, "|")
    for (k = 1; k <= n; k++)
        printf "[%s]=%s ", a[k], (a[k] + 0)
    print ""
    CONVFMT = "%d"
    s = -nan ""
    print s
    OFMT = "%.2f"
    print inf, 0.5
}
