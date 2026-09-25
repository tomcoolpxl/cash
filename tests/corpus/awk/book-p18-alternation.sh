# source: https://github.com/onetrueawk/awk/blob/master/testdir/p.18
# desc: /(apple|cherry) (pie|tart)/ grouping and alternation
printf 'apple pie\ncherry tart\napple tart\nbanana pie\ncherry  pie\n' | awk '/(apple|cherry) (pie|tart)/'
