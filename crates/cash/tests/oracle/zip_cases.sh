# zip cases, run under Info-ZIP's zip 3.0 (the oracle) and under cash's builtin, the
# archives looked at with each side's own unzip and zipinfo (checked against UnZip 6.00
# by unzip_cases.sh). What zip says and does is checked byte for byte, and so are stored
# archives without extra fields (-0 -X), which zip makes the same everywhere. Deflate's
# own bytes differ between zlib-like encoders, so the compressed sizes are left out of
# listings, and the files are ones every encoder compresses to the same percentage.
# Owners, symbolic links and unreadable files depend on the system, so they are left to
# the integration test.
# zip_cases.out is zip 3.0's output.
#
# Regenerate the golden file (WSL; setsid: no terminal to ask a password at):
#   setsid -w bash zip_cases.sh > zip_cases.out 2>&1

exec 2>&1 </dev/null
export LC_ALL=C TZ=UTC0
unset ZIP ZIPOPT UNZIP UNZIPOPT ZIPINFO ZIPINFOOPT
umask 002
dir=$(mktemp -d)
cd "$dir" || exit 1
rc() { echo "rc=$?"; }
# zipinfo without the lines compression changes: the archive's size and the totals.
zi() { zipinfo "$@" | sed '/^Zip file size/d; /bytes compressed/d'; }
stamp() { touch -d '2020-01-02 03:04:05' "$@"; }

mkdir -p src/sub r/one/two
printf 'hello\n' > src/a.txt
printf 'world\n' > src/sub/b.txt
head -c 3000 /dev/zero | tr '\0' a > src/big.txt
printf '\000\001\002binary' > src/bin.dat
printf 'deep\n' > r/one/two/f
stamp src/a.txt src/sub/b.txt src/big.txt src/bin.dat src/sub src r/one/two/f r/one/two r/one r

echo "== stored, the bytes"; zip -0 -X s.zip src/a.txt src/sub/b.txt src/bin.dat; rc; sha256sum < s.zip; wc -c < s.zip
zip -0 -X sd.zip src src/sub src/sub/b.txt; rc; sha256sum < sd.zip; zipinfo sd.zip
zip -X x.zip src/a.txt; rc; sha256sum < x.zip; zipinfo -v x.zip | sed -n '/Central directory entry/,$p'
echo "== add and list"; zip a.zip src/a.txt src/big.txt src/bin.dat; rc; unzip -l a.zip; zi a.zip; unzip -tq a.zip
echo "== recurse"; zip -r r.zip r; rc; unzip -Z1 r.zip; zip -rD rd.zip r; rc; unzip -Z1 rd.zip; zip -rq rj.zip r -j; unzip -Z1 rj.zip
zip dir.zip r; rc; unzip -Z1 dir.zip
echo "== junk, exclude, include"; zip -j j.zip src/a.txt src/sub/b.txt; rc; unzip -Z1 j.zip
zip xi.zip src/a.txt src/bin.dat src/sub/b.txt -x '*.dat'; rc; unzip -Z1 xi.zip; zip ii.zip src/a.txt src/bin.dat -i '*.txt'; rc; unzip -Z1 ii.zip
zip -r rx.zip r -x 'r/one/two/*'; rc; unzip -Z1 rx.zip
echo "== names"; cd src; zip ../n.zip ../src/a.txt ./sub/b.txt; rc; cd ..; unzip -Z1 n.zip; zip -q abs.zip "$PWD/src/a.txt"; unzip -Z1 abs.zip | grep -c '^[^/].*/src/a.txt$'
echo "== update, freshen, delete"; cp a.zip u.zip; zip u.zip src/a.txt; rc; zip -u u.zip src/a.txt src/sub/b.txt; rc
touch -d '2021-01-01' src/a.txt; zip -f u.zip src/a.txt src/sub/b.txt; rc; zip -f u.zip src/a.txt; rc; zip -u u.zip src/a.txt; rc; stamp src/a.txt
zip -d u.zip 'src/b*' src/sub/b.txt; rc; zip -d u.zip nosuch; rc; unzip -Z1 u.zip; zip u.zip src/sub/b.txt src/a.txt src/bin.dat; rc; unzip -Z1 u.zip
cp s.zip fs.zip; zip -FS fs.zip src/a.txt src/bin.dat; rc; unzip -Z1 fs.zip
zip -q del.zip src/a.txt; zip -d del.zip src/a.txt; rc; unzip -l del.zip; rc
echo "== nothing to do"; zip c.zip; rc; zip c.zip nosuch; rc; zip -MM mm.zip src/a.txt nosuch; rc; test -e mm.zip || echo none; zip -d nosuch.zip a; rc
echo "== errors"; zip -Y x2.zip src/a.txt; rc; zip --nosuch x2.zip src/a.txt; rc; zip -b; rc; zip -Z foo x2.zip src/a.txt; rc
printf 'junk' > junk.zip; zip junk.zip src/a.txt; rc; zip nodir/x.zip src/a.txt; rc; zip -t 13132020 x2.zip src/a.txt; rc
echo "== suffixes"; zip -q noext src/a.txt; ls noext*; cp src/big.txt big.zip; cp src/big.txt big.gz; stamp big.zip big.gz
zip sfx.zip big.zip big.gz; rc; zi sfx.zip; zip -n .gz sfx2.zip big.zip big.gz; rc; zi sfx2.zip
echo "== standard input and output"; printf 'piped\n' | zip -q p.zip -; rc; unzip -p p.zip -; unzip -Z p.zip | sed -n 3p | cut -c1-38
zip - src/a.txt src/big.txt | cat > o.zip; zi o.zip; unzip -tq o.zip; printf 'filter\n' | zip | cat > f.zip; unzip -p f.zip
echo "== comments"; printf 'the comment\nline 2\n' | zip -z c2.zip src/a.txt; rc; unzip -z c2.zip; printf 'one\n.\ntwo\n' | zip -z c2.zip; rc; unzip -z c2.zip
printf 'first\nsecond\n' | zip -c ec.zip src/a.txt src/sub/b.txt; rc; zipinfo -v ec.zip | grep -A2 'file comment begins'
echo "== encryption"; zip -P pw e.zip src/a.txt src/big.txt; rc; unzip -P pw -tq e.zip; unzip -P pw -p e.zip src/a.txt; zi e.zip; zip -e e2.zip src/a.txt; rc
echo "== bzip2 and levels"; zip -Z bzip2 bz.zip src/big.txt src/a.txt; rc; zi bz.zip; unzip -p bz.zip src/big.txt | cksum
zip -q -1 l1.zip src/big.txt; zipinfo -v l1.zip | grep 'sub-type'; zip -q -9 l9.zip src/big.txt; unzip -v l9.zip | sed -n 4p | cut -c1-16
echo "== move, test, latest"; cp src/a.txt mv.txt; stamp mv.txt; zip -m m.zip mv.txt; rc; test -e mv.txt || echo gone; zip -T t.zip src/a.txt; rc
zip -o o2.zip src/a.txt; rc; stat -c %Y o2.zip
echo "== show files"; zip -sf a2.zip src/a.txt; rc; zip -q a2.zip src/a.txt src/sub/b.txt; zip -sf a2.zip; rc; zip -sf a2.zip src/a.txt src/big.txt; rc
echo "== dates and names from input"; zip -t 01012021 tt.zip src/a.txt; rc; zip -tt 2021-01-01 tt.zip src/a.txt; rc; printf 'src/a.txt\nsrc/sub/b.txt\n' | zip at.zip -@; rc
echo "== verbose"; zip -v -0 v.zip src/a.txt src/big.txt; rc
echo "== help"; zip -h | tail -3; rc

cd / && rm -rf "$dir"
