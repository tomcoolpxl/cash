# diff cases, run under GNU diffutils' diff (the oracle) and under cash's builtin.
# diff_cases.out is GNU diffutils 3.12's output. Output that holds bytes a reader cannot
# see (escape sequences, carriage returns) is shown through od. The time stamps of the
# unified and context headers are masked with sed where they appear: they are the
# files' modification times, which no two runs share.
#
# Regenerate the golden file (WSL):
#   TZ=UTC LC_ALL=C.UTF-8 bash diff_cases.sh > diff_cases.out 2>&1

exec 2>&1 < /dev/null
export TZ=UTC LC_ALL=C.UTF-8
dir=$(mktemp -d)
cd "$dir" || exit 1
show() { od -An -c | sed 's/  */ /g; s/ *$//'; }
# The header lines of -u and -c, without their time stamps.
mask() { sed 's/^\(---\|+++\|\*\*\*\) \([^	]*\)	.*/\1 \2 [T]/'; }

printf 'a\nb\nc\n' > a; printf 'a\nB\nc\nd\n' > b; cp a same
printf '1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11\n12\n13\n14\n15\n16\n17\n18\n19\n20\n' > q1
sed 's/^10$/ten/; s/^15$/fifteen/' q1 > q2
printf 'a\nb\nc\nd\ne\n' > e1; printf 'a\nc\nx\nd\ne\nf\n' > e2

echo "== normal"; diff a b; echo "rc=$?"
echo "== identical"; diff a same; echo "rc=$?"
echo "== identical -s"; diff -s a same; echo "rc=$?"
echo "== -q"; diff -q a b; echo "rc=$?"; diff -q a same; echo "rc=$?"
echo "== -q -s"; diff -qs a same; diff --brief --report-identical-files a b
echo "== normal several"; diff e1 e2; diff q1 q2
echo "== -u"; diff -u a b | mask; echo "rc=$?"
echo "== -U 1 and -U0"; diff -U 1 q1 q2 | mask; diff -u0 e1 e2 | mask; diff --unified=0 a b | mask
echo "== -u far apart"; diff -u q1 q2 | mask
echo "== -c"; diff -c a b | mask; echo "rc=$?"
echo "== -C 1 and -c0"; diff -C 1 a b | mask; diff -c0 e1 e2 | mask; diff --context=1 q1 q2 | mask
echo "== -e"; diff -e a b; diff -e e1 e2
echo "== -L"; diff -u -L left a b | mask; diff -u -L left -L right a b; diff -c --label L1 --label=L2 a b; diff -q -L X -L Y a b; diff -s -L X a same
echo "== -y"; diff -y a b | show; echo "rc=$?"
echo "== -y -W 40 and 20"; diff -y -W 40 a b | show; diff -y --width=20 e1 e2 | show
echo "== -y --suppress-common-lines --left-column"; diff -y -W 30 --suppress-common-lines e1 e2 | show; diff -y -W 30 --left-column a b | show
echo "== -y -t"; diff -y -t -W 30 a b | show
printf 'abcdefghijklmnopqrstuvwxyz\n' > long1; printf 'abcdefghijklmnopqrstuvwxyZ\n' > long2
echo "== -y long lines"; diff -y -W 30 long1 long2 | show
printf 'a\tb\n' > tab1; printf 'a\tc\n' > tab2
echo "== -y tabs in the text"; diff -y -W 30 tab1 tab2 | show
echo "== -y colour"; diff -y -W 30 --color=always a b | show

echo "== -i"; printf 'Hello\nx\n' > i1; printf 'hello\ny\n' > i2; diff -i i1 i2; echo "rc=$?"; diff --ignore-case i1 i2 >/dev/null; echo "rc=$?"
echo "== -w"; printf 'a b\nc\n' > w1; printf 'ab\n c \n' > w2; diff -w w1 w2; echo "rc=$?"; diff w1 w2; echo "rc=$?"
echo "== -b"; printf 'b \n' > s1; printf 'b\n' > s2; printf '  b\n' > s3; printf ' b\n' > s4; printf 'a b\n' > s5; printf 'ab\n' > s6; printf 'a  b\n' > s7
diff -b s1 s2; echo "rc=$?"; diff -b s3 s4; echo "rc=$?"; diff -b s4 s2; echo "rc=$?"; diff -b s5 s6; echo "rc=$?"; diff -b s5 s7; echo "rc=$?"
echo "== -B"; printf 'x\n\ny\nz\n' > bl1; printf 'x\ny\nz\nw\n' > bl2; printf 'x\n\ny\n' > bl3; printf 'x\ny\n' > bl4
diff -B bl1 bl2; echo "rc=$?"; diff -B bl3 bl4; echo "rc=$?"; diff -u -B bl1 bl2 | mask; diff -c -B bl1 bl2 | mask
echo "== -B whitespace-only line"; printf 'x\n  \ny\n' > bl5; diff -B bl5 bl4; echo "rc=$?"; diff -B -b bl5 bl4; echo "rc=$?"
echo "== -B near a real change"; printf '1\n2\n3\n4\n\n5\n6\n7\n8\n9x\n10\n' > bl6; head -10 q1 > bl7; diff -u -B bl7 bl6 | mask; diff -U1 -B bl7 bl6 | mask
echo "== -I"; printf 'x\n# c1\n# d1\ny\n' > r1; printf 'x\n# c2\n# d2\ny\n' > r2; diff -I '^# c' r1 r2; echo "rc=$?"; diff -I '^# c' -I '^# d' r1 r2; echo "rc=$?"
echo "== -E"; printf '\ta\n' > t1; printf '        a\n' > t2; printf '    a\n' > t3; printf 'a\tb\n' > t4; printf 'a       b\n' > t5
diff -E t1 t2; echo "rc=$?"; diff -E t1 t3; echo "rc=$?"; diff -E t4 t5; echo "rc=$?"; diff t4 t5; echo "rc=$?"
echo "== -a and -t"; diff -a -t t4 t5; diff --expand-tabs --tabsize=4 t1 t3; echo "rc=$?"
echo "== -T"; diff -T a b | show; diff -u -T a b | mask | show; diff -c -T a b | mask | show

echo "== CRLF both"; printf 'a\r\nb\r\n' > crlf1; printf 'a\r\nc\r\n' > crlf2; diff crlf1 crlf2 | show; echo "rc=$?"
echo "== CRLF against LF"; printf 'a\nb\n' > lf1; diff crlf1 lf1 | show; echo "rc=$?"
echo "== CRLF against LF with --strip-trailing-cr, -w, -b"; diff --strip-trailing-cr crlf1 lf1; echo "rc=$?"; diff -w crlf1 lf1; echo "rc=$?"; diff -b crlf1 lf1; echo "rc=$?"
echo "== --strip-trailing-cr output is stripped"; diff --strip-trailing-cr crlf2 lf1 | show; diff -u --strip-trailing-cr crlf2 lf1 | mask | show
echo "== a lone CR at the end stays"; printf 'a\r' > cr3; printf 'a' > cr4; diff --strip-trailing-cr cr3 cr4 | show; echo "rc=$?"

echo "== no newline at end"; printf 'a\nb' > n1; printf 'a\nb\n' > n2; printf 'a\nc' > n3
diff n1 n2; diff -u n1 n2 | mask; diff -c n1 n2 | mask; diff n1 n3; diff -y -W 30 n1 n2 | show; diff -y -W 30 n2 n3 | show
echo "== no newline, -w and -b"; diff -w n1 n2; echo "rc=$?"; diff -b n1 n2; echo "rc=$?"
echo "== empty files"; : > z1; : > z2; diff z1 z2; echo "rc=$?"; diff z1 a; echo "rc=$?"; diff -u z1 a | mask; diff -u a z1 | mask; diff -s z1 z2

echo "== colour normal"; diff --color=always a b | show
echo "== colour unified"; diff --color=always -u -L a -L b a b | show
echo "== colour context"; diff --color=always -c -L a -L b a b | show
echo "== colour with no newline"; diff --color=always n1 n2 | show
echo "== --color=never and --color on a pipe"; diff --color=never a b | show; diff --color a b | show
echo "== colour identical"; diff --color=always a same | show; echo "rc=$?"

echo "== binary"; printf 'a\0b\n' > bin1; printf 'a\0c\n' > bin2; cp bin1 bin3
diff bin1 bin2; echo "rc=$?"; diff bin1 bin3; echo "rc=$?"; diff -q bin1 bin2; echo "rc=$?"; diff -s bin1 bin3; echo "rc=$?"; diff bin1 a; echo "rc=$?"
echo "== binary with -a"; diff -a bin1 bin2 | show; echo "rc=$?"

echo "== stdin"; printf 'a\nb\n' | diff - a; echo "rc=$?"; printf 'a\nb\n' | diff -u a - | mask; echo "rc=$?"; printf 'a\nb\nc\n' | diff a -; echo "rc=$?"
echo "== missing"; diff nosuch a; echo "rc=$?"; diff a nosuch; echo "rc=$?"; diff nosuch nosuch2; echo "rc=$?"
echo "== missing with -N"; diff -N nosuch a; echo "rc=$?"; diff -N -u a nosuch | mask; echo "rc=$?"; diff -c -N nosuch a | mask

mkdir -p d1/sub d2/sub d2/only2 d1/onlydir; printf 'same\n' > d1/sub/s; printf 'same\n' > d2/sub/s; printf 'one\n' > d1/f; printf 'two\n' > d2/f; printf 'x\n' > d1/only1
echo "== directories without -r"; diff d1 d2; echo "rc=$?"
echo "== -r"; diff -r d1 d2; echo "rc=$?"
echo "== -r with a trailing slash"; diff -r d1/ d2/; echo "rc=$?"
echo "== -ru and -r -u"; diff -ru d1 d2 | mask; diff -r -u d1 d2 | mask; diff d1 d2 -r -U 1 | mask
echo "== -rq and -rs"; diff -rq d1 d2; echo "rc=$?"; diff -rs d1 d2; echo "rc=$?"
echo "== -rN"; diff -rN d1 d2; echo "rc=$?"; diff -rNu d1 d2 | mask
echo "== -N without -r keeps a lone directory"; diff -N d1 d2
echo "== identical directories"; diff -r d1/sub d2/sub; echo "rc=$?"; diff -rs d1/sub d2/sub; echo "rc=$?"
echo "== dir vs file"; mkdir -p d3; printf 'zz\n' > d3/f; diff d3/f d3; echo "rc=$?"; diff d1/f d3; echo "rc=$?"; diff d3 d1/f; echo "rc=$?"
echo "== dir vs stdin"; printf 'q\n' | diff - d1; echo "rc=$?"
echo "== dir vs missing file"; diff d1 nosuchfile; echo "rc=$?"
echo "== file on one side, directory on the other"; mkdir -p fd1 fd2/f; printf 'q\n' > fd1/f; diff -r fd1 fd2; echo "rc=$?"; diff -rN fd1 fd2; echo "rc=$?"
echo "== -rN with a subdirectory missing"; mkdir -p md1/sub md2; printf 'z\n' > md1/sub/z; diff -rN md1 md2; echo "rc=$?"; diff -r md1 md2; echo "rc=$?"
echo "== -r binary"; mkdir -p rb1 rb2; printf 'a\0' > rb1/bin; printf 'b\0' > rb2/bin; diff -r rb1 rb2; echo "rc=$?"; diff -rq rb1 rb2
echo "== -x"; mkdir -p x1 x2; printf '1\n' > x1/keep.c; printf '1\n' > x1/skip.o; printf '2\n' > x2/keep.c; diff -r -x '*.o' x1 x2; echo "rc=$?"; diff -r --exclude='*.o' x1 x2 | head -1
echo "== sorted by bytes"; mkdir -p o1 o2; for n in B a C d; do echo $n > o1/$n; done; diff -r o1 o2
echo "== nested only"; mkdir -p q3/d/e q4; printf '1\n' > q3/d/e/f; diff -r q3 q4

echo "== bad option"; diff --foo a b; echo "rc=$?"; diff -Z1 a b; echo "rc=$?"; diff -z a b; echo "rc=$?"
echo "== missing operand"; diff a; echo "rc=$?"; diff; echo "rc=$?"
echo "== extra operand"; diff a b a; echo "rc=$?"
echo "== bad context, width, tabsize, colour"; diff -U x a b; echo "rc=$?"; diff -W x a b; echo "rc=$?"; diff --tabsize=0 a b; echo "rc=$?"; diff --color=foo a b; echo "rc=$?"
echo "== conflicting formats"; diff -u -c a b; echo "rc=$?"; diff -y -u a b; echo "rc=$?"
echo "== too many labels"; diff -L a -L b -L c a b; echo "rc=$?"
echo "== an option needing an argument"; diff -L; echo "rc=$?"; diff --label; echo "rc=$?"; diff --brief=1 a b; echo "rc=$?"
echo "== -NUM"; diff -1 -u q1 q2 | mask | head -3; diff -u1 q1 q2 | mask | head -3
echo "== --help head"; diff --help | head -2
echo "== --version"; diff --version | head -1; diff -v | head -1

cd / && rm -rf "$dir"
