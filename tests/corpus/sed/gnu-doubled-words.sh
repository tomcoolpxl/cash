# source: https://www.gnu.org/software/sed/manual/html_node/Text-search-across-multiple-lines.html
# desc: find doubled words on one line and across lines with \b \w \s
# tags: gnu-ext (\b, \w, \s)
printf 'It was the best of times,\nit was the worst of times,\nit was the the age of wisdom,\n' > dup1.txt
sed -En '/\b(\w+)\s+\1\b/{=;p}' dup1.txt
printf 'It was the best of times, it was the\nworst of times, it was the\nthe age of wisdom,\nit was the age of foolishness,\n' > dup2.txt
sed -En '{N; /\b(\w+)\s+\1\b/{=;p} ; D}' dup2.txt
