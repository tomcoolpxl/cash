# source: https://www.pement.org/awk/awk1line.txt (SELECTIVE PRINTING)
# desc: head, head -1, tail -2, tail -1
printf 'l%s\n' 1 2 3 4 5 6 7 8 9 10 11 12 > f
awk 'NR < 11' f; echo --
awk 'NR>1{exit};1' f; echo --
awk '{y=x "\n" $0; x=$0};END{print y}' f; echo --
awk 'END{print}' f
