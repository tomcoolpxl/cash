# source: https://www.gnu.org/software/sed/manual/html_node/cat-_002db.html
# desc: cat -b (number non-blank lines)
cat > catb.sed <<'EOF'
/^$/ {
  p
  b
}

# Same as cat -n from now
x
/^$/ s/^.*$/1/
G
h
s/^/      /
s/^ *\(......\)\n/\1  /p
x
s/\n.*$//
/^9*$/ s/^/0/
s/.9*$/x&/
h
s/^.*x//
y/0123456789/1234567890/
x
s/x.*$//
G
s/\n//
h
EOF
printf 'a\n\nb\n\n\nc\n' | sed -n -f catb.sed
