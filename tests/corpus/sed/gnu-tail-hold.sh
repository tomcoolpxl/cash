# source: https://www.gnu.org/software/sed/manual/html_node/tail.html
# desc: print the last 10 lines keeping a window in hold space
printf '%s\n' '1! {; H; g; }' '1,10 !s/[^\n]*\n//' '$p' 'h' > tail1.sed
printf 'l%s\n' 1 2 3 4 5 6 7 8 9 10 11 12 13 | sed -n -f tail1.sed
