# source: https://www.gnu.org/software/sed/manual/html_node/cat-_002dn.html
# desc: cat -n via sed -e "=" piped into a multi-line sed script
printf 'a\nb\nc\n' > f
sed -e "=" f | sed -e '
  s/^/      /
  N
  s/^ *\(......\)\n/\1  /
'
