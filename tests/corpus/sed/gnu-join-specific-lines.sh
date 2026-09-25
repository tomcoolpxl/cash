# source: https://www.gnu.org/software/sed/manual/html_node/Joining-lines.html
# desc: join lines 2 and 3
printf 'hello\nhel\nlo\nhello\n' | sed '2{N;s/\n//;}'
