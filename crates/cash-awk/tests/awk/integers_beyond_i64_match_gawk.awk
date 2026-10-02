BEGIN {
    print 2^53, 2^63, 2^64, 1e20, 1e30, -2^64
    f = 1; for (i = 1; i <= 25; i++) f *= i; print f
    printf "[%d][%d][%d][%5d][%-25d|][%025d][%+d][%i]\n", 2^64, -1e30, 1e20, 42, 2^70, 2^64, 2^64, 1e19
    printf "[%x][%X][%o][%u][%x][%u]\n", -1, -255, -8, -1, 255, 2^40
    x = 2^64; print x ""
    a[2^64] = 1; for (k in a) print "key", k
}
