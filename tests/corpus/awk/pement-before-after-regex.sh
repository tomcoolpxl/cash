# source: https://www.pement.org/awk/awk1line.txt (SELECTIVE PRINTING)
# desc: print the line before a regex (two methods) and after a regex (getline)
printf 'regex first\nb\nc\nregex d\ne\n' > f
awk '/regex/{print x};{x=$0}' f; echo --
awk '/regex/{print (NR==1 ? "match on line 1" : x)};{x=$0}' f; echo --
awk '/regex/{getline;print}' f
