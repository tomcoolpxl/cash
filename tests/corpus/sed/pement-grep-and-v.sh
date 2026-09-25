# source: https://www.pement.org/sed/sed1line.txt (SELECTIVE PRINTING)
# desc: emulate grep and grep -v, both methods each
printf 'one\nregexp two\nthree\nregexp four\n' > f
sed -n '/regexp/p' f; sed '/regexp/!d' f
sed -n '/regexp/!p' f; sed '/regexp/d' f
