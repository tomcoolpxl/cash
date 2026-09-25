# source: https://www.pement.org/sed/sed1line.txt (SELECTIVE DELETION)
# desc: delete ALL blank lines, method 1 and 2
printf 'a\n\nb\n\n\nc\n' > f
sed '/^$/d' f; sed '/./!d' f
