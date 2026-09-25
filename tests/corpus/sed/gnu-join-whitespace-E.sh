# source: https://www.gnu.org/software/sed/manual/html_node/Joining-lines.html
# desc: join lines that start with whitespace (SMTP headers) using -E and \s
# tags: gnu-ext (\s, one-liner labels)
printf 'Subject: Hello\n    World\nContent-Type: multipart/alternative;\n    boundary=94eb\nTo: Jane\n' | sed -E ':a ; $!N ; s/\n\s+/ / ; ta ; P ; D'
