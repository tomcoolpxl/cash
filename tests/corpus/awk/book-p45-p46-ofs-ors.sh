# source: https://github.com/onetrueawk/awk/blob/master/testdir/p.45 and p.46
# desc: OFS/ORS set in BEGIN; concatenation without OFS
awk 'BEGIN	{ OFS = ":" ; ORS = "\n\n" }
	{ print $1, $2 }' countries | head -n 6
awk '	{ print $1 $2 }' countries
