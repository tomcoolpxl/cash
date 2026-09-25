# source: https://www.gnu.org/software/sed/manual/html_node/cat-_002dn.html
# desc: cat -n using both buffers and y/// to increment
cat > catn.sed <<'EOF'
# Prime the pump on the first line
x
/^$/ s/^.*$/1/

# Add the correct line number before the pattern
G
h

# Format it and print it
s/^/      /
s/^ *\(......\)\n/\1  /p

# Get the line number from hold space; add a zero
# if we're going to add a digit on the next line
g
s/\n.*$//
/^9*$/ s/^/0/

# separate changing/unchanged digits with an x
s/.9*$/x&/

# keep changing digits in hold space
h
s/^.*x//
y/0123456789/1234567890/
x

# keep unchanged digits in pattern space
s/x.*$//

# compose the new number, remove the newline implicitly added by G
G
s/\n//
h
EOF
printf 'l%s\n' 1 2 3 4 5 6 7 8 9 10 11 12 | sed -n -f catn.sed
