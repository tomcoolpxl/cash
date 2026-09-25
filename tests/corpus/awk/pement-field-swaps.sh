# source: https://www.pement.org/awk/awk1line.txt (TEXT CONVERSION)
# desc: print fields in opposite order; switch fields (no print); delete field 2; reverse fields
printf 'one two three\nfour five\n' > file
awk '{print $2, $1}' file; echo --
awk '{temp = $1; $1 = $2; $2 = temp}' file; echo --
awk '{ $2 = ""; print }' file; echo --
awk '{for (i=NF; i>0; i--) printf("%s ",$i);print ""}' file | sed -n l
