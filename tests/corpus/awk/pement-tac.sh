# source: https://www.pement.org/awk/awk1line.txt (TEXT CONVERSION)
# desc: reverse order of lines (emulates tac)
printf 'one\ntwo\n\nfour\n' > file1
awk '{a[i++]=$0} END {for (j=i-1; j>=0;) print a[j--] }' file*
