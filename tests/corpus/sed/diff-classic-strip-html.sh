# source: tests/sed-differential.sh, case classic-strip-html, frozen into the corpus
# desc: classic-strip-html
printf '%b' '<p>This is <b>bold</b> and <a href="#">a link</a>.</p>\n' | sed 's/<[^>]*>//g'
