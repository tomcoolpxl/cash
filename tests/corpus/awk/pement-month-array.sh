# source: https://www.pement.org/awk/awk1line.txt (ARRAY CREATION)
# desc: create month[] with split and mdigit[] indexed by strings
awk 'BEGIN {
 split("Jan Feb Mar Apr May Jun Jul Aug Sep Oct Nov Dec", month, " ")
 for (i=1; i<=12; i++) mdigit[month[i]] = i
 print month[1], month[12], mdigit["Mar"], mdigit["Dec"]
}'
