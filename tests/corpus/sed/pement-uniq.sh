# source: https://www.pement.org/sed/sed1line.txt (SELECTIVE DELETION)
# desc: delete duplicate, consecutive lines (emulates uniq)
printf 'a\na\nb\nc\nc\nc\na\n' | sed '$!N; /^\(.*\)\n\1$/!P; D'
