# source: https://github.com/onetrueawk/awk/blob/master/testdir/p.47
# desc: print > "file" redirection split into two files
awk '$3 > 100	{ print >"tempbig" }
$3 <= 100	{ print >"tempsmall" }' countries
cat tempbig; echo --; cat tempsmall
