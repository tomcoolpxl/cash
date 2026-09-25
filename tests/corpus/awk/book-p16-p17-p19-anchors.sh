# source: https://github.com/onetrueawk/awk/blob/master/testdir/p.16, p.17, p.19
# desc: /^.$/, $2 !~ /^[0-9]+$/, dynamic regex string in a variable
printf 'x\nxy\nA 12\nB 1a\n' > f
awk '/^.$/' f; echo --
awk '$2 !~ /^[0-9]+$/' f; echo --
awk 'BEGIN	{ digits = "^[0-9]+$" }
$2 !~ digits' f
