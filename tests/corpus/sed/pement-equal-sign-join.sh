# source: https://www.pement.org/sed/sed1line.txt (TEXT CONVERSION)
# desc: if a line begins with =, append it to the previous line replacing = with a space
printf 'alpha\n=beta\n=gamma\ndelta\n=eps\n' | sed -e :a -e '$!N;s/\n=/ /;ta' -e 'P;D'
