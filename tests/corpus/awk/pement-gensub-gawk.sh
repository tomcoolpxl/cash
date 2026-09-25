# source: https://www.pement.org/awk/awk1line.txt (TEXT CONVERSION)
# desc: replace only the 4th instance with gensub
# tags: gawk-ext (gensub)
printf 'foo foo foo foo foo\n' | gawk '{$0=gensub(/foo/,"bar",4)}; 1'
