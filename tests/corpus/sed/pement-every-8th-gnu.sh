# source: https://www.pement.org/sed/sed1line.txt (SELECTIVE DELETION)
# desc: delete every 8th line (GNU first~step)
# tags: gnu-ext (0~8 address)
printf '%s\n' 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 | sed '0~8d'
