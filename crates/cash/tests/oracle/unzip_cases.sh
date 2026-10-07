# unzip and zipinfo cases, run under Info-ZIP's UnZip 6.00 (the oracle) and under cash's
# builtins. The archives are Info-ZIP zip 3.0's (and Python's, for an MS-DOS host and
# names that climb out), made once and kept here as hex: what is listed, tested,
# extracted and said is checked byte for byte. Owners and symbolic links depend on the
# system, so they are left to the integration test.
# unzip_cases.out is UnZip 6.00's output.
#
# Regenerate the golden file (WSL; setsid: no terminal to ask a password at):
#   setsid -w bash unzip_cases.sh > unzip_cases.out 2>&1

exec 2>&1 </dev/null
export LC_ALL=C TZ=UTC0
unset UNZIP UNZIPOPT ZIPINFO ZIPINFOOPT
dir=$(mktemp -d)
cd "$dir" || exit 1
rc() { echo "rc=$?"; }
unhex() { printf "$(printf '%s' "$1" | sed 's/../\\x&/g')"; }

# a.zip: src/, src/a.txt, src/big.txt (deflated), src/bin.dat, src/sub/, src/sub/b.txt,
# with a comment; e.zip: two of them encrypted with "pw"; bz.zip: bzip2; l.zip: a
# symbolic link; empty.zip: no entries; evil.zip: absolute and climbing names; fat.zip:
# made on MS-DOS, one read-only.
unhex 504b03040a00000000008318225000000000000000000000000004001c007372632f5554090003a55d0d5ea55d0d5e75780b000104e803000004e8030000504b03040a00000000008318225020303a36060000000600000009001c007372632f612e7478745554090003a55d0d5ea55d0d5e75780b000104e803000004e803000068656c6c6f0a504b030414000000080083182250c83cdc5e15000000b80b00000b001c007372632f6269672e7478745554090003a55d0d5ea55d0d5e75780b000104e803000004e8030000edc13101000000c2a0aceb5fc21a1e40010000ef06504b03040a000000000083182250bf4d1eca09000000090000000b001c007372632f62696e2e6461745554090003a55d0d5ea55d0d5e75780b000104e803000004e803000000010262696e617279504b03040a00000000008318225000000000000000000000000008001c007372632f7375622f5554090003a55d0d5ea55d0d5e75780b000104e803000004e8030000504b03040a000000000083182250a86138dd06000000060000000d001c007372632f7375622f622e7478745554090003a55d0d5ea55d0d5e75780b000104e803000004e8030000776f726c640a504b01021e030a000000000083182250000000000000000000000000040018000000000000001000fd41000000007372632f5554050003a55d0d5e75780b000104e803000004e8030000504b01021e030a00000000008318225020303a360600000006000000090018000000000001000000b4813e0000007372632f612e7478745554050003a55d0d5e75780b000104e803000004e8030000504b01021e0314000000080083182250c83cdc5e15000000b80b00000b0018000000000001000000b481870000007372632f6269672e7478745554050003a55d0d5e75780b000104e803000004e8030000504b01021e030a000000000083182250bf4d1eca09000000090000000b0018000000000000000000b481e10000007372632f62696e2e6461745554050003a55d0d5e75780b000104e803000004e8030000504b01021e030a000000000083182250000000000000000000000000080018000000000000001000fd412f0100007372632f7375622f5554050003a55d0d5e75780b000104e803000004e8030000504b01021e030a000000000083182250a86138dd06000000060000000d0018000000000001000000b481710100007372632f7375622f622e7478745554050003a55d0d5e75780b000104e803000004e8030000504b05060000000006000600dc010000be01000009006120636f6d6d656e74 > a.zip
unhex 504b03040a00090000008318225020303a36120000000600000009001c007372632f612e7478745554090003a55d0d5e69bdc56a75780b000104e803000004e80300000648ebbb9fb120531e3097424cd259dff611504b070820303a361200000006000000504b03040a000900000083182250a86138dd12000000060000000d001c007372632f7375622f622e7478745554090003a55d0d5e69bdc56a75780b000104e803000004e803000038396c3f53fbe563b67b246719521132dd59504b0708a86138dd1200000006000000504b01021e030a00090000008318225020303a361200000006000000090018000000000001000000b481000000007372632f612e7478745554050003a55d0d5e75780b000104e803000004e8030000504b01021e030a000900000083182250a86138dd12000000060000000d0018000000000001000000b481650000007372632f7375622f622e7478745554050003a55d0d5e75780b000104e803000004e8030000504b05060000000002000200a2000000ce0000000000 > e.zip
unhex 504b03042e0000000c0083182250c83cdc5e2d000000b80b00000b001c007372632f6269672e7478745554090003a55d0d5e69bdc56a75780b000104e803000004e8030000425a68363141592653599a1ff8560000058108a00000008008200030cc089a4a5003c5dc914e14242687fe1580504b01021e032e0000000c0083182250c83cdc5e2d000000b80b00000b0018000000000000000000b481000000007372632f6269672e7478745554050003a55d0d5e75780b000104e803000004e8030000504b0506000000000100010051000000720000000000 > bz.zip
unhex 504b03040a000000000083182250baf7ebc1050000000500000008001c007372632f6c696e6b5554090003a55d0d5ea55d0d5e75780b000104e803000004e8030000612e747874504b03040a00000000008318225020303a36060000000600000009001c007372632f612e7478745554090003a55d0d5e69bdc56a75780b000104e803000004e803000068656c6c6f0a504b01021e030a000000000083182250baf7ebc10500000005000000080018000000000000000000ffa1000000007372632f6c696e6b5554050003a55d0d5e75780b000104e803000004e8030000504b01021e030a00000000008318225020303a360600000006000000090018000000000001000000b481470000007372632f612e7478745554050003a55d0d5e75780b000104e803000004e8030000504b050600000000020002009d000000900000000000 > l.zip
unhex 504b0506000000000000000000000000000000000000 > empty.zip
unhex 504b0304140000000000831822501f934a0d0400000004000000060000002f6162732f786162730a504b03041400000000008318225041e391620300000003000000070000002e2e2f75702f7975700a504b0304140000000000831822509d6adc7402000000020000000a0000006f6b2f2e2e2f2e2e2f7a7a0a504b01021403140000000000831822501f934a0d0400000004000000060000000000000000000000a481000000002f6162732f78504b010214031400000000008318225041e391620300000003000000070000000000000000000000a481280000002e2e2f75702f79504b01021403140000000000831822509d6adc7402000000020000000a0000000000000000000000a481500000006f6b2f2e2e2f2e2e2f7a504b05060000000003000300a10000007a0000000000 > evil.zip
unhex 504b0304140000000000831822509f8019ca050000000500000007000000444f532e545854646f730d0a504b030414000000000083182250000000000000000000000000040000004449522f504b0304140000000000831822505d9bb08f02000000020000000700000052554e2e4558454d5a504b01021400140000000000831822509f8019ca0500000005000000070000000000000000002100000000000000444f532e545854504b010214001400000000008318225000000000000000000000000004000000000000000000100000002a0000004449522f504b01021400140000000000831822505d9bb08f020000000200000007000000000000000000200000004c00000052554e2e455845504b050600000000030003009c000000730000000000 > fat.zip

echo "== list"; unzip -l a.zip; rc; unzip -v a.zip; rc; unzip -lq a.zip; unzip -lqq a.zip
unzip -l a.zip 'src/*.txt' nosuch; rc; unzip -l a.zip SRC/A.TXT; rc; unzip -C -l a.zip SRC/A.TXT; rc
unzip -W -l a.zip '*.txt'; rc; unzip -W -l a.zip 'src/*.txt'; unzip -l a.zip -x 'src/sub/*'; rc; unzip -l a.zip src/; rc
echo "== zipinfo"; zipinfo a.zip; rc; zipinfo -l a.zip; zipinfo -m a.zip; zipinfo -1 a.zip; zipinfo -2 -h -t a.zip
zipinfo -h a.zip; zipinfo -t a.zip; zipinfo -T a.zip src/a.txt; zipinfo -z a.zip; zipinfo a.zip nosuch; rc
unzip -Z1 a.zip; unzip -Z -s a.zip src/sub/b.txt; zipinfo a.zip -x 'src/s*'
echo "== zipinfo -v"; zipinfo -v a.zip src/a.txt src/big.txt src/sub/
echo "== other archives"; zipinfo bz.zip; zipinfo e.zip; zipinfo l.zip; zipinfo fat.zip; zipinfo evil.zip; unzip -v bz.zip; unzip -v e.zip
echo "== comment"; unzip -z a.zip; rc; unzip -z bz.zip; rc
echo "== test"; unzip -t a.zip; rc; unzip -tq a.zip; rc; unzip -tqq a.zip; rc; unzip -t bz.zip; rc; unzip -t a.zip src/a.txt nosuch; rc
echo "== extract"; mkdir x; cd x; unzip ../a.zip; rc; find . | sort; stat -c '%a %Y %n' src/a.txt src/big.txt src/bin.dat src/sub src; cat src/a.txt; cksum src/big.txt src/bin.dat
echo "== extract again"; unzip ../a.zip; rc; unzip -n ../a.zip; rc; unzip -o ../a.zip src/a.txt; rc; unzip -q -o ../a.zip; rc
printf 'n\ny\nN\n' | unzip ../a.zip; rc; printf 'A\n' | unzip ../a.zip; rc
echo "== freshen and update"; unzip -f ../a.zip; rc; printf 'old\n' > src/a.txt; touch -d 2019-01-01 src/a.txt; unzip -fo ../a.zip; rc; cat src/a.txt
rm src/big.txt; unzip -uo ../a.zip; rc; unzip -f ../a.zip src/nosuch; rc; cd ..
echo "== -d and -j"; unzip -d out a.zip src/a.txt src/sub/b.txt; rc; find out | sort; unzip -d deep/er a.zip src/a.txt; rc
unzip -j -d flat a.zip; rc; find flat | sort; unzip -o -d out/ a.zip src/a.txt; rc
echo "== to standard output"; unzip -p a.zip src/a.txt src/sub/b.txt; rc; unzip -c a.zip src/a.txt src/big.txt | head -c 120; echo; unzip -p bz.zip | cksum
echo "== not matched"; unzip a.zip nosuch -d nm; rc; unzip a.zip src/a.txt nosuch -d nm; rc; unzip -d nm a.zip -x nosuch src/a.txt; rc
echo "== password"; unzip -P pw -t e.zip; rc; unzip -P wrong -t e.zip; rc; unzip -P pw -p e.zip src/a.txt; rc; unzip -t e.zip; rc
mkdir pw; unzip -P pw -d pw e.zip; rc; cat pw/src/sub/b.txt; unzip -P wrong -d pw2 e.zip; rc
echo "== symbolic link"; mkdir lk; cd lk; unzip ../l.zip; rc; cd ..
echo "== names"; mkdir ev; cd ev; unzip ../evil.zip; rc; find . | sort; cd ..; mkdir ft; cd ft; unzip ../fat.zip; rc; find . | sort; stat -c '%a %n' DOS.TXT; cd ..
echo "== empty and broken"; unzip -l empty.zip; rc; unzip empty.zip; rc; zipinfo empty.zip; rc; printf 'junk' > junk.zip; unzip -l junk.zip; rc; zipinfo junk; rc
unzip nosuch; rc; unzip -l nosuch.zip; rc; zipinfo nosuch; rc
echo "== extra bytes"; { printf '#!/bin/sh\nexit\n'; cat a.zip; } > sfx.zip; unzip -l sfx.zip; rc; zipinfo -1 sfx.zip; rc; unzip -tq sfx.zip; rc; unzip -p sfx.zip src/a.txt; rc
echo "== bad CRC"; { head -c 129 a.zip; printf 'J'; tail -c +131 a.zip; } > bad.zip; unzip -t bad.zip; rc; mkdir bc; unzip -d bc bad.zip src/a.txt; rc; cat bc/src/a.txt; unzip -p bad.zip src/a.txt; rc
# A wildcard in the archive's name, expanded by unzip itself; one archive each, since
# the order of several is the folder's.
echo "== wildcard archives"; unzip -l 'a*.zip'; rc; unzip -tq 'jun*.zip'; rc; unzip -tq 'nosuch*.zip'; rc; zipinfo -1 'bz*.zip'; rc
unzip -t a.zip src/a.txt; rc; unzip -tq a.zip 'src/*.txt'; rc
echo "== options"; unzip -Y a.zip > u.txt; rc; tail -3 u.txt; unzip > u.txt; rc; tail -3 u.txt; zipinfo -Y a.zip > u.txt; rc; tail -2 u.txt
unzip -n -o -l a.zip | head -3; unzip -l -d somewhere a.zip | head -2; UNZIP=-qq unzip -l a.zip | head -2; unzip -d; rc
cp a.zip t.zip; unzip -T t.zip; rc; stat -c %Y t.zip; unzip -l a | head -2; unzip -Z -1 a src/a.txt
cd / && rm -rf "$dir"
