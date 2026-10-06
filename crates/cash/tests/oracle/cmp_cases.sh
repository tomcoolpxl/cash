# cmp cases, run under GNU diffutils' cmp (the oracle) and under cash's builtin.
# cmp_cases.out is GNU diffutils 3.12's output.
#
# Regenerate the golden file (WSL):
#   TZ=UTC LC_ALL=C.UTF-8 bash cmp_cases.sh > cmp_cases.out 2>&1

exec 2>&1 < /dev/null
export TZ=UTC LC_ALL=C.UTF-8
dir=$(mktemp -d)
cd "$dir" || exit 1

printf 'abc\ndef\n' > c1; printf 'abc\ndXf\n' > c2; printf 'abc\n' > c3; : > c4; cp c1 same
echo "== differing"; cmp c1 c2; echo "rc=$?"
echo "== identical"; cmp c1 same; echo "rc=$?"; cmp c1 c1; echo "rc=$?"
echo "== -b"; cmp -b c1 c2; echo "rc=$?"; cmp --print-bytes c1 c2
echo "== -l"; cmp -l c1 c2; echo "rc=$?"; cmp --verbose c1 c2
echo "== -lb and -bl"; cmp -lb c1 c2; cmp -bl c1 c2; cmp -l -b c1 c2
echo "== -s"; cmp -s c1 c2; echo "rc=$?"; cmp -s c1 same; echo "rc=$?"; cmp --quiet c1 c2; echo "rc=$?"; cmp --silent c1 c2; echo "rc=$?"
echo "== EOF"; cmp c1 c3; echo "rc=$?"; cmp c3 c1; echo "rc=$?"
echo "== EOF on an empty file"; cmp c4 c3; echo "rc=$?"; cmp c3 c4; echo "rc=$?"; cmp c4 c4; echo "rc=$?"
echo "== EOF in a line"; printf 'ab\ncd\n' > l1; printf 'ab\nc' > l2; cmp l1 l2; echo "rc=$?"; cmp -l l1 l2; echo "rc=$?"; cmp -s l1 l2; echo "rc=$?"
echo "== -l with EOF"; cmp -l c1 c3; echo "rc=$?"; cmp -lb c3 c1; echo "rc=$?"
echo "== -i N"; cmp -i 4 c1 c2; echo "rc=$?"; cmp --ignore-initial=4 c1 c2; cmp -i4 c1 c2
echo "== -i N:M"; cmp -i 1:5 c1 c2; echo "rc=$?"; cmp -i 0:0 c1 c2; cmp --ignore-initial=4:4 c1 c2; echo "rc=$?"
echo "== positional skips"; cmp c1 c2 4; echo "rc=$?"; cmp c1 c2 1 5; echo "rc=$?"; cmp -i 4 c1 c2 1 5; echo "rc=$?"
echo "== -n N"; cmp -n 5 c1 c2; echo "rc=$?"; cmp -n 6 c1 c2; echo "rc=$?"; cmp --bytes=6 c1 c2; echo "rc=$?"; cmp -n5 c1 c3; echo "rc=$?"
echo "== -n with EOF"; cmp -n 100 c1 c3; echo "rc=$?"; cmp -n 4 c1 c3; echo "rc=$?"
echo "== suffixes"; printf 'aaaaaaaaaaaaaaaa' > w1; printf 'aaaaaaaaaaaaaaab' > w2; cmp -i 1k w1 w2; echo "rc=$?"; cmp -n 1K w1 w2; echo "rc=$?"; cmp -i 2kB w1 w2; echo "rc=$?"
echo "== -l widths"; cmp -l w1 w2; printf 'aaaaaaaaab' > w3; cmp -l w1 w3; echo "rc=$?"; cmp -l w3 w1
echo "== -b high bytes"; printf '\377\n' > h1; printf '\177\n' > h2; printf '\001\n' > h3; printf ' \n' > h4; cmp -b h1 h2; cmp -lb h1 h2; cmp -b h3 h4; cmp -lb h3 h4
echo "== differing lengths, long"; seq 1 3000 > big1; seq 1 3000 | sed 's/^2999$/x/' > big2; cmp big1 big2; echo "rc=$?"; cmp -l big1 big2; head -c 5000 big1 > big3; cmp big1 big3; echo "rc=$?"
echo "== stdin"; printf 'abc\n' | cmp - c3; echo "rc=$?"; printf 'abd\n' | cmp c3; echo "rc=$?"; printf 'abd\n' | cmp c3 -; echo "rc=$?"; printf 'abc\n' | cmp -l - c3; echo "rc=$?"
echo "== missing"; cmp nosuch c1; echo "rc=$?"; cmp c1 nosuch; echo "rc=$?"; cmp -s nosuch c1; echo "rc=$?"
echo "== directory"; mkdir -p d; cmp d c1; echo "rc=$?"; cmp c1 d; echo "rc=$?"
echo "== bad usage"; cmp; echo "rc=$?"; cmp --foo c1 c2; echo "rc=$?"; cmp -z c1 c2; echo "rc=$?"; cmp -l -s c1 c2; echo "rc=$?"; cmp -ls c1 c2; echo "rc=$?"
echo "== bad values"; cmp -i x c1 c2; echo "rc=$?"; cmp -n x c1 c2; echo "rc=$?"; cmp --bytes= c1 c2; echo "rc=$?"; cmp c1 c2 1 2 3; echo "rc=$?"; cmp -i; echo "rc=$?"
echo "== --help head"; cmp --help | head -2
echo "== --version"; cmp --version | head -1; cmp -v | head -1

cd / && rm -rf "$dir"
