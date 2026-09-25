# source: https://www.pement.org/awk/awk1line.txt (SELECTIVE PRINTING)
# desc: regex to EOF (,0 and ,EOF), NR range, NR==N, NR==N exit, Iowa/Montana range
printf 'Ohio\nIowa\nKansas\nMontana\nTexas\nregex\nUtah\nIowa\nZ\n' > f
awk '/regex/,0' f; echo --
awk '/regex/,EOF' f; echo --
awk 'NR==2,NR==4' f; echo --
awk 'NR==5' f; awk 'NR==5 {print;exit}' f; echo --
awk '/Iowa/,/Montana/' f
