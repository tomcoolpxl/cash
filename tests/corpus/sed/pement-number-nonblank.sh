# source: https://www.pement.org/sed/sed1line.txt (NUMBERING)
# desc: number each line of file, but only print numbers if line is not blank
printf 'alpha\n\nbeta\n\n\ngamma\n' > filename
sed '/./=' filename | sed '/./N; s/\n/ /'
