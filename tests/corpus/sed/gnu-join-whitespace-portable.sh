# source: https://www.gnu.org/software/sed/manual/html_node/Joining-lines.html
# desc: the portable (non-gnu) variation of the SMTP header join
printf 'Subject: Hello\n    World\nContent-Type: multipart/alternative;\n    boundary=94eb\nTo: Jane\n' | sed -e :a -e '$!N;s/\n  */ /;ta' -e 'P;D'
