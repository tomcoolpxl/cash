# source: https://www.pement.org/sed/sed1line.txt (SELECTIVE PRINTING / DELETION)
# desc: print / delete section between two regular expressions (inclusive)
printf 'Ohio\nIowa\nKansas\nMontana\nTexas\nIowa\nUtah\n' > f
sed -n '/Iowa/,/Montana/p' f
echo --
sed '/Iowa/,/Montana/d' f
