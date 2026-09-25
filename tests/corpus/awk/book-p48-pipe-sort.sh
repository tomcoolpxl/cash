# source: https://github.com/onetrueawk/awk/blob/master/testdir/p.48
# desc: print into | "sort" pipe from END
awk 'BEGIN	{ FS = "\t" }
	{ pop[$4] += $3 }
END	{ for (c in pop)
		print c ":" pop[c] | "sort" }' countries
