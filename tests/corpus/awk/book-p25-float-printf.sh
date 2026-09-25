# source: https://github.com/onetrueawk/awk/blob/master/testdir/p.25
# desc: population density with %6.1f
awk '{ printf "%10s %6.1f\n", $1, 1000 * $3 / $2 }' countries
