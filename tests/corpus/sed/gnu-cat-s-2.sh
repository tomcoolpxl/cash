# source: https://www.gnu.org/software/sed/manual/html_node/cat-_002ds.html
# desc: squeeze blank lines, second script (1,/^./ range)
cat > cats2.sed <<'EOF'
# delete all leading empty lines
1,/^./{
/./!d
}

# on an empty line we remove it and all the following
# empty lines, but one
:x
/./!{
N
s/^\n$//
tx
}
EOF
printf '\n\na\n\n\nb\nc\n\n' | sed -f cats2.sed | sed -n l
