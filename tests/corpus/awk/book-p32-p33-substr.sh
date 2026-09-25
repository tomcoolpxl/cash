# source: https://github.com/onetrueawk/awk/blob/master/testdir/p.32 and p.33
# desc: assign $1 = substr($1,1,3) (record rebuild); accumulate substrings
awk '{ $1 = substr($1, 1, 3); print }' countries; echo --
awk '	{ s = s " " substr($1, 1, 3) }
END	{ print s }' countries
