# source: https://github.com/onetrueawk/awk/blob/master/testdir/p.14 and p.15
# desc: /\$/ and /\\/ regex escapes
printf 'cost $5\nback\\slash\nplain\n' > f
awk '/\$/' f; echo --
awk '/\\/' f
