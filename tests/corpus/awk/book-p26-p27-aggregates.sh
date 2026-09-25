# source: https://github.com/onetrueawk/awk/blob/master/testdir/p.26, p.26a, p.27
# desc: sum and count with line continuation in print; max tracking
awk '/Asia/	{ pop = pop + $3; n = n + 1 }
END	{ print "population of", n,\
		"Asian countries in millions is", pop }' countries
awk '/Asia/	{ pop += $3; ++n }
END	{ print "population of", n,\
		"Asian countries in millions is", pop }' countries
awk 'maxpop < $3	{ maxpop = $3; country = $1 }
END		{ print country, maxpop }' countries
