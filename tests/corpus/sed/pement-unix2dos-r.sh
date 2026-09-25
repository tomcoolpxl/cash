# source: https://www.pement.org/sed/sed1line.txt (TEXT CONVERSION)
# desc: convert LF to CR/LF with \r in replacement
# tags: gnu-ext (\r in replacement)
printf 'one\ntwo\n' | sed 's/$/\r/' | tr '\r' R
