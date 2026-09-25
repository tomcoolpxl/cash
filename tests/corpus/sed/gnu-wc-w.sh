# source: https://www.gnu.org/software/sed/manual/html_node/wc-_002dw.html
# desc: count words (TAB in bracket written literally, "bx;" branches)
t=$(printf '\t')
printf '%s\n' "s/[ $t][ $t]*/ /g" 's/^/ /' 's/ [^ ][^ ]*/a /g' 's/ //g' 'H' 'x' 's/\n//' \
 '/aaaaaaaaaa/! bx;   s/aaaaaaaaaa/b/g' '/bbbbbbbbbb/! bx;   s/bbbbbbbbbb/c/g' \
 '/cccccccccc/! bx;   s/cccccccccc/d/g' '/dddddddddd/! bx;   s/dddddddddd/e/g' \
 '/eeeeeeeeee/! bx;   s/eeeeeeeeee/f/g' '/ffffffffff/! bx;   s/ffffffffff/g/g' \
 '/gggggggggg/! bx;   s/gggggggggg/h/g' 's/hhhhhhhhhh//g' ':x' '$! { h; b; }' ':y' \
 '/a/! s/[b-h]*/&0/' 's/aaaaaaaaa/9/' 's/aaaaaaaa/8/' 's/aaaaaaa/7/' 's/aaaaaa/6/' \
 's/aaaaa/5/' 's/aaaa/4/' 's/aaa/3/' 's/aa/2/' 's/a/1/' 'y/bcdefgh/abcdefg/' '/[a-h]/ by' 'p' > wcw.sed
printf 'one two\tthree\n  four   five six seven eight nine ten eleven\n' | sed -n -f wcw.sed
