# source: cash (PI-20/LANG-25: printf %q's table against Bash's sh_backslash_quote)
# desc: %q and ${x@Q} for every printable ASCII punctuation mark, inside, first and last
for c in ' ' '!' '"' '#' '$' '%' '&' "'" '(' ')' '*' '+' ',' '-' '.' '/' ':' ';' '<' '=' '>' '?' '@' '[' '\' ']' '^' '_' '`' '{' '|' '}' '~'; do
  printf '%s => %q %q %q\n' "$c" "a${c}b" "${c}a" "a${c}"
done
x='a b'; echo "${x@Q}"
printf '%q\n' 'a=~b' 'a:~b' 'a~b'
