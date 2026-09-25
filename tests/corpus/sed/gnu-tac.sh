# source: https://www.gnu.org/software/sed/manual/html_node/tac.html
# desc: tac workalike as a -n script file with spaces after addresses
printf '%s\n' '1! G' '$ p' 'h' > tac.sed
printf 'one\ntwo\nthree\n' | sed -n -f tac.sed
