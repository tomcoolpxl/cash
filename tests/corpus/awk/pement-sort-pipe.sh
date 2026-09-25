# source: https://www.pement.org/awk/awk1line.txt (TEXT CONVERSION; /etc/passwd replaced by a local file)
# desc: print and sort the login names of all users (print | "sort")
printf 'root:x:0:0\nzed:x:1:1\nalice:x:2:2\nbob:x:3:3\n' > passwd
awk -F ":" '{print $1 | "sort" }' passwd
