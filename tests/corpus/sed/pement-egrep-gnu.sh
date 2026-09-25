# source: https://www.pement.org/sed/sed1line.txt (SELECTIVE PRINTING)
# desc: grep for AAA or BBB or CCC using \| (GNU sed only)
# tags: gnu-ext (\| in BRE)
printf 'AAA\nxx\nBBB\nCCC\n' | sed '/AAA\|BBB\|CCC/!d'
