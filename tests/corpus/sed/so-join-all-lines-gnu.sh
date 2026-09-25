# source: https://stackoverflow.com/questions/1251999/how-can-i-replace-each-newline-n-with-a-space-using-sed
# desc: the famous sed ':a;N;$!ba;s/\n/ /g' (one-liner label syntax)
# tags: gnu-ext (label terminated by ';')
printf 'a\nb\nc\nd\n' | sed ':a;N;$!ba;s/\n/ /g'
