# source: https://www.pement.org/sed/sed1line.txt (SELECTIVE PRINTING)
# desc: print paragraph if it contains AAA or BBB or CCC (GNU sed only)
# tags: gnu-ext (\| in BRE, one-liner '}' followed by ';')
printf 'p1 AAA\nmore\n\np2 none\nx\n\np3 BBB\n' | sed '/./{H;$!d;};x;/AAA\|BBB\|CCC/b;d'
