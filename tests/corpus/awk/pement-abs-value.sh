# source: https://www.pement.org/awk/awk1line.txt (NUMBERING AND CALCULATIONS)
# desc: replace each field with its absolute value, two methods (field rebuild with OFS)
printf -- '-1  2   -3\n4 -5.5 x\n' > f
awk '{for (i=1; i<=NF; i++) if ($i < 0) $i = -$i; print }' f
awk '{for (i=1; i<=NF; i++) $i = ($i < 0) ? -$i : $i; print }' f
