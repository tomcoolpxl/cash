# source: https://www.gnu.org/software/sed/manual/html_node/tail.html
# desc: tail with a sliding N/D window
printf '%s\n' '1h' '2,10 {; H; g; }' '$q' '1,9d' 'N' 'D' > tail2.sed
printf 'l%s\n' 1 2 3 4 5 6 7 8 9 10 11 12 13 | sed -f tail2.sed
