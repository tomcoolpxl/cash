# source: https://www.pement.org/awk/awk1line.txt (NUMBERING AND CALCULATIONS)
# desc: print the largest first field and the line that contains it (string/number comparison)
printf '5 five\n10 ten\n9 nine\n' | awk '$1 > max {max=$1; maxline=$0}; END{ print max, maxline}'
printf 'pear x\napple y\nzoo z\n' | awk '$1 > max {max=$1; maxline=$0}; END{ print max, maxline}'
