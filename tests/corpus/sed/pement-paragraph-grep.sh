# source: https://www.pement.org/sed/sed1line.txt (SELECTIVE PRINTING)
# desc: print paragraph if it contains AAA / AAA and BBB and CCC / AAA or BBB or CCC
printf 'p1 AAA\nmore\n\np2 none\nx\n\np3 BBB\nCCC AAA\n' > f
sed -e '/./{H;$!d;}' -e 'x;/AAA/!d;' f
echo --
sed -e '/./{H;$!d;}' -e 'x;/AAA/!d;/BBB/!d;/CCC/!d' f
echo --
sed -e '/./{H;$!d;}' -e 'x;/AAA/b' -e '/BBB/b' -e '/CCC/b' -e d f
