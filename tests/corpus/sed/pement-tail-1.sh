# source: https://www.pement.org/sed/sed1line.txt (SELECTIVE PRINTING)
# desc: print the last line (emulates tail -1), both methods
printf 'a\nb\nc\n' > f
sed '$!d' f
sed -n '$p' f
