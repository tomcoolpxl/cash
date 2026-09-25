# source: https://github.com/onetrueawk/awk/blob/master/testdir/p.30 and p.31
# desc: bare length, length($1) max
awk '{ print length, $0 }' countries; echo --
awk 'length($1) > max	{ max = length($1); name = $1 }
END			{ print name }' countries
