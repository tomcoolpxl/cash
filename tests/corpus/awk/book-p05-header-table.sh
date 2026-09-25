# source: https://github.com/onetrueawk/awk/blob/master/testdir/p.5
# desc: FS set in BEGIN plus printf-formatted table
awk 'BEGIN	{ FS = "\t"
	  printf "%10s %6s %5s %15s\n", "COUNTRY", "AREA", "POP", "CONTINENT" }
	{ printf "%10s %6d %5d %15s\n", $1, $2, $3, $4 }' countries
