# source: https://www.pement.org/sed/sed1line.txt (SELECTIVE PRINTING)
# desc: print only lines of 65 characters or longer / less than 65
printf '%070d\n%064d\n%065d\nshort\n' 1 2 3 > f
sed -n '/^.\{65\}/p' f
echo --
sed -n '/^.\{65\}/!p' f
echo --
sed '/^.\{65\}/d' f
