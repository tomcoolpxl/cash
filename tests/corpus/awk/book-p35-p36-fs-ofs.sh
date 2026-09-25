# source: https://github.com/onetrueawk/awk/blob/master/testdir/p.35 and p.36
# desc: FS = OFS = "\t"; field assignment; create $5
awk 'BEGIN			{ FS = OFS = "\t" }
$4 ~ /^North America$/	{ $4 = "NA" }
$4 ~ /^South America$/	{ $4 = "SA" }
			{ print }' countries | tr '\t' '|'
echo --
awk 'BEGIN	{ FS = OFS = "\t" }
	{ $5 = 1000 * $3 / $2 ; print $1, $2, $3, $4, $5 }' countries | tr '\t' '|'
