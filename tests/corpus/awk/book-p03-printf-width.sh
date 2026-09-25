# source: https://github.com/onetrueawk/awk/blob/master/testdir/p.3
# desc: printf with %10s and %-16d
awk '{ printf "[%10s] [%-16d]\n", $1, $3 }' countries
