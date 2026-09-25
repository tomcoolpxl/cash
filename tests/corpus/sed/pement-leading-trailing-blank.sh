# source: https://www.pement.org/sed/sed1line.txt (SELECTIVE DELETION)
# desc: delete all leading blank lines; delete all trailing blank lines (two methods)
printf '\n\na\n\nb\n\n\n' > f
sed '/./,$!d' f | sed -n l; echo --
sed -e :a -e '/^\n*$/{$d;N;ba' -e '}' f | sed -n l; echo --
sed -e :a -e '/^\n*$/N;/\n$/ba' f | sed -n l
