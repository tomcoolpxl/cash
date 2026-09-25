# source: https://www.pement.org/sed/sed1line.txt (TEXT CONVERSION)
# desc: add commas to numeric strings with \B and \> (GNU sed)
# tags: gnu-ext (\B, \> word boundaries)
printf '1234567\n12\n1234\n' | sed ':a;s/\B[0-9]\{3\}\>/,&/;ta'
