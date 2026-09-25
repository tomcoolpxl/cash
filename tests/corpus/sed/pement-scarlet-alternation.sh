# source: https://www.pement.org/sed/sed1line.txt (TEXT CONVERSION)
# desc: change scarlet or ruby or puce to red using \| (GNU sed only)
# tags: gnu-ext (\| alternation in BRE)
printf 'scarlet ruby puce pink\n' | sed 's/scarlet\|ruby\|puce/red/g'
