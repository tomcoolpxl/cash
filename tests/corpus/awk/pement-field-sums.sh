# source: https://www.pement.org/awk/awk1line.txt (NUMBERING AND CALCULATIONS)
# desc: sums of the fields of every line, and of all fields in all lines
printf '1 2 3\n10 20\n-5 5.5\n' > f
awk '{s=0; for (i=1; i<=NF; i++) s=s+$i; print s}' f
awk '{for (i=1; i<=NF; i++) s=s+$i}; END{print s}' f
