# source: https://github.com/onetrueawk/awk/blob/master/testdir/p.38, p.39, p.40
# desc: if block, while loop over fields, for loop over fields
awk '{	if (maxpop < $3) {
		maxpop = $3
		country = $1
	}
}
END	{ print country, maxpop }' countries
printf 'a b\nc\n' | awk '{	i = 1
	while (i <= NF) {
		print $i
		i++
	}
}'
printf 'a b\nc\n' | awk '{	for (i = 1; i <= NF; i++)
		print $i
}'
