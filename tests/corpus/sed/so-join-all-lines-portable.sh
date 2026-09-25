# source: https://stackoverflow.com/questions/1251999/how-can-i-replace-each-newline-n-with-a-space-using-sed
# desc: cross-platform form of the join-all-lines trick with -e separated labels
printf 'a\nb\nc\nd\n' | sed -e ':a' -e 'N' -e '$!ba' -e 's/\n/ /g'
