# source: https://github.com/onetrueawk/awk/blob/master/testdir/p.29
# desc: gsub(/USA/, "United States") then print
awk '	{ gsub(/USA/, "United States"); print }' countries
