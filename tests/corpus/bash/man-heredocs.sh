# source: https://www.gnu.org/software/bash/manual/bash.html#Here-Documents (cash's own case)
# desc: here-documents quoted and not, <<- with tabs, expansions inside, several on a line, in functions and command substitutions
name=world
cat <<EOF
plain $name $(echo sub) $((1+2)) \$escaped \\ backslash
EOF
cat <<'EOF'
quoted $name $(echo sub) \$ \\
EOF
cat <<"E O F"
spaced delimiter $name
E O F
cat <<-EOF
	tabs stripped $name
		both tabs stripped
	EOF
cat <<EOF; cat <<EOF2
first
EOF
second
EOF2
f() {
    cat <<END
in a function: $1
END
}
f arg
x=$(cat <<EOF
inside a substitution
EOF
)
echo "[$x]"
cat <<EOF | tr a-z A-Z
piped $name
EOF
while read -r line; do echo "read: $line"; done <<EOF
r1
r2
EOF
cat <<EOF
line with trailing backslash \
joined
EOF
cat <<'EOF'
no join \
here
EOF
cat <<<"here-string $name"
cat <<< 'quoted here-string $name'
