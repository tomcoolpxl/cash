# source: https://www.pement.org/sed/sed1line.txt (TEXT CONVERSION)
# desc: reverse each character on the line (emulates rev)
printf 'hello world\n12345\nx\n\n' | sed '/\n/!G;s/\(.\)\(.*\n\)/&\2\1/;//D;s/.//'
