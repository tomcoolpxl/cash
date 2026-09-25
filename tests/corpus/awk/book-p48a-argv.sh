# source: https://github.com/onetrueawk/awk/blob/master/testdir/p.48a
# desc: ARGC/ARGV in BEGIN, then exit
awk 'BEGIN {
	for (i = 1; i < ARGC; i++)
		printf "%s ", ARGV[i]
	printf "\n"
	exit
}' one 'two words' x=1 three
