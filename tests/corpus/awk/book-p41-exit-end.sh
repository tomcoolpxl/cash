# source: https://github.com/onetrueawk/awk/blob/master/testdir/p.41
# desc: exit in main rule still runs END; FILENAME in END
awk 'NR >= 10	{ exit }
END		{ if (NR < 10)
			print FILENAME " has only " NR " lines" }' countries
head -n 3 countries > short
awk 'NR >= 10	{ exit }
END		{ if (NR < 10)
			print FILENAME " has only " NR " lines" }' short
