# source: https://www.pement.org/sed/sed1line.txt (SELECTIVE DELETION)
# desc: delete all lines except duplicate lines (emulates uniq -d)
printf 'a\na\nb\nc\nc\nc\nd\n' | sed '$!N; s/^\(.*\)\n\1$/\1/; t; D'
