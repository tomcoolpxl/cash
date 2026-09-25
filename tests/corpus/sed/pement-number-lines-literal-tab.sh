# source: https://www.pement.org/sed/sed1line.txt (NUMBERING; "USE OF '\t'" note says press TAB instead)
# desc: number each line, portable form with a literal TAB in the replacement
printf 'alpha\nbeta\ngamma\n' > filename
tab=$(printf '\t')
sed = filename | sed "N;s/\n/$tab/"
