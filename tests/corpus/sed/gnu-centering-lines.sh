# source: https://www.gnu.org/software/sed/manual/html_node/Centering-lines.html
# desc: center all lines of a file on an 80 column width (script file, y/TAB/ /)
t=$(printf '\t')
printf '%s\n' '1 {' '  x' '  s/^$/          /' '  s/^.*$/&&&&&&&&/' '  x' '}' "y/$t/ /" 's/^ *//' 's/ *$//' 'G' 's/^\(.\{81\}\).*$/\1/' 's/^\(.*\)\n\(.*\)\2/\2\1/' > center.sed
printf 'hello\n\tcentered text  \n' | sed -f center.sed | sed -n l
