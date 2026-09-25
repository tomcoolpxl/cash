# source: https://github.com/onetrueawk/awk/blob/master/testdir/p.44
# desc: recursive factorial function
printf '1\n5\n10\n20\n' | awk 'function fact(n) {
	if (n <= 1)
		return 1
	else
		return n * fact(n-1)
}
{ print $1 "! is " fact($1) }'
