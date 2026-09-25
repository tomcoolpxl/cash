# source: https://www.pement.org/sed/sed1line.txt (TEXT CONVERSION)
# desc: join pairs of lines side-by-side (like paste)
printf '1\n2\n3\n4\n5\n' | sed '$!N;s/\n/ /'
