# source: https://www.pement.org/sed/sed1line.txt (TEXT CONVERSION)
# desc: convert CR/LF to LF using \x0D
# tags: gnu-ext (\xHH escape)
printf 'one\r\ntwo\r\n' | sed 's/\x0D$//' | tr '\r' R
