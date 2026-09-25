# source: https://www.pement.org/sed/sed1line.txt (SPECIAL APPLICATIONS)
# desc: remove most HTML tags (accommodates multiple-line tags)
printf '<p>Hello <b>bold</b> <a\nhref="x">link</a></p>\n' | sed -e :a -e 's/<[^>]*>//g;/</N;//ba'
