# source: https://www.pement.org/sed/sed1line.txt (SELECTIVE PRINTING)
# desc: beginning at line 3, print every 7th line (GNU first~step)
# tags: gnu-ext (3~7 address)
printf 'line%s\n' 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 | sed -n '3~7p'
