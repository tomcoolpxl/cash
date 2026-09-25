# source: https://www.pement.org/sed/sed1line.txt (SELECTIVE PRINTING)
# desc: grep for AAA and BBB and CCC in any order / in that order / egrep emulation
printf 'CCC BBB AAA\nAAA BBB CCC\nAAA only\nBBB only\nnone\n' > f
sed '/AAA/!d; /BBB/!d; /CCC/!d' f
echo --
sed '/AAA.*BBB.*CCC/!d' f
echo --
sed -e '/AAA/b' -e '/BBB/b' -e '/CCC/b' -e d f
