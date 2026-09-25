# source: https://www.pement.org/awk/awk1line.txt (STRING CREATION; same program without the gawk flag)
# desc: insert 49 spaces after column 6 using an interval expression (POSIX ERE)
printf 'abcdefghij\n' | awk 'BEGIN{while(a++<49)s=s " "};{sub(/^.{6}/,"&" s)};1' | sed -n l
