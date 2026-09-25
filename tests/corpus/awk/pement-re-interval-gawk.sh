# source: https://www.pement.org/awk/awk1line.txt (STRING CREATION)
# desc: insert 49 spaces after column 6 using gawk --re-interval
# tags: gawk-ext (--re-interval option)
printf 'abcdefghij\n' | gawk --re-interval 'BEGIN{while(a++<49)s=s " "};{sub(/^.{6}/,"&" s)};1'
