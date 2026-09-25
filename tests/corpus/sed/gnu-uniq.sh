# source: https://www.gnu.org/software/sed/manual/html_node/uniq.html
# desc: make duplicate lines unique with a 2-line N/P/D window
cat > uniq.sed <<'EOF'
h

:b
# On the last line, print and exit
$b
N
/^\(.*\)\n\1$/ {
    # The two lines are identical.  Undo the effect of
    # the n command.
    g
    bb
}

# If the N command had added the last line, print and exit
$b

# The lines are different; print the first and go
# back working on the second.
P
D
EOF
printf 'a\na\nb\nc\nc\nc\nd\n' | sed -f uniq.sed
