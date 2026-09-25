# source: https://github.com/onetrueawk/awk/blob/master/testdir/p.34
# desc: $2 /= 1000 (OFMT/CONVFMT number output)
awk '{ $2 /= 1000; print }' countries
