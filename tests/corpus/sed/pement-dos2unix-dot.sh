# source: https://www.pement.org/sed/sed1line.txt (TEXT CONVERSION)
# desc: convert DOS newlines (CR/LF) to Unix format, assumes all lines end with CR/LF
# tags: cash-divergence (D49: s/.$// names no CR, so it removes the last visible character)
printf 'one\r\ntwo\r\n' | sed 's/.$//' | tr '\r' R
