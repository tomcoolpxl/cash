# source: https://github.com/onetrueawk/awk/blob/master/testdir/p.2 (The AWK Programming Language test programs)
# desc: { print $1, $3 } over the countries file
awk '{ print $1, $3 }' countries
