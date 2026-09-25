# source: https://www.pement.org/awk/awk1line.txt (NUMBERING AND CALCULATIONS)
# desc: number each line of a file (number on left, right-aligned)
printf 'x\ny\nz\n' | awk '{printf("%5d : %s\n", NR,$0)}'
