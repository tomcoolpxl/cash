# source: https://github.com/onetrueawk/awk/blob/master/testdir/p.10 and p.37
# desc: $1 == $4 (Australia) and $1 "" == $2 "" string coercion
awk '$1 == $4' countries; echo --
printf '10 10.0\n10 10\n1e1 10\n' | awk '$1 "" == $2 ""'; echo --
printf '10 10.0\n10 10\n1e1 10\n' | awk '$1 == $2'
