# source: https://www.gnu.org/software/sed/manual/html_node/Joining-lines.html
# desc: join backslash-continued lines (manual notes it requires GNU sed)
# tags: gnu-ext (label and branch terminated without newline)
printf 'this \\\nis \\\na \\\nlong \\\nline\nand another \\\nline\n' | sed -e ':x /\\$/ { N; s/\\\n//g ; bx }'
