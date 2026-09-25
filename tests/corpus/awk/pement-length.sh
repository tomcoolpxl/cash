# source: https://www.pement.org/awk/awk1line.txt (SELECTIVE PRINTING)
# desc: lines longer than 64 / shorter than 64 (bare length)
printf '%070d\n%063d\n%064d\nshort\n' 1 2 3 > f
awk 'length > 64' f; echo --
awk 'length < 64' f
