# source: https://github.com/dylanaraps/pure-bash-bible#string-lists
# desc: lists, nesting, prefixes and suffixes, ranges inside lists, and braces that stay literal
echo {apples,oranges,pears,grapes}
echo pre-{a,b,c}-post
echo {a,b{1,2},c}
echo {x,y}{1..3}
echo {1..3},{a,b}
echo {a..e..2} {10..1..3} {05..10..2}
echo {a} {} {,x} {a,} "{a,b}" \{a,b\}
echo {a..c}{,}
dirs=(Movies Music ISOS)
mkdir -p dl/{Movies,Music,ISOS,Keep}
rm -rf dl/{Movies,Music,ISOS}
echo dl/*
