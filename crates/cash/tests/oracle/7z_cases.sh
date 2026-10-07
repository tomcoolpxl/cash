# 7z cases, run under 7-Zip 26.03 (the oracle: Scoop's 7z.exe on Windows) and under
# cash's builtin. The archives are 7-Zip's own, kept here as hex: stored.7z (-mx0
# -mhc=off of d/), solid.7z (d/ and text.txt, LZMA2), aes.7z (the same with -psecret
# -mhe=on). What is listed, tested, extracted and said is checked line for line.
# 7z_cases.out is 7-Zip's output.
#
# 7-Zip on Windows ends lines with CRLF and shows `\` in names; cash's 7z writes LF and
# `/` (the user's choice, 2026-10-07), so `z` turns 7-Zip's into those. 7-Zip shows
# times in Windows' own zone and ignores TZ: the fixtures' times, and those the script
# sets, are 08:00 and 09:00 UTC, 10:00 and 11:00 in Brussels, which cash shows in UTC
# with TZ=UTC0. Archives 7-Zip compresses differ from cash's in their sizes, so `zl`
# shows a listing's fields that do not depend on them; stored ones (-mx0 -mhc=off) are
# compared byte for byte.
#
# Regenerate the golden file (on Windows, with Scoop's 7-Zip and a cash without the
# builtin, such as 1.9.0), then shift the Brussels times to UTC:
#   cash 7z_cases.sh | sed 's/2026-10-07 10:00:00/2026-10-07 08:00:00/;
#     s/2026-10-07 11:00:00/2026-10-07 09:00:00/' > 7z_cases.out

exec 2>&1 </dev/null
export TZ=UTC0
dir=$(mktemp -d)
cd "$dir" || exit 1
z() { 7z "$@" > "$dir/o.txt" 2>&1; r=$?; tr -d '\r' < "$dir/o.txt" | sed 's#\\#/#g'; echo "rc=$r"; }
zl() {
  z l -slt "$@" | grep -E '^(Path|Size|Modified|Attributes|CRC|Method|Solid|Blocks|Block|Encrypted) = |^rc='
}
zt() { z t "$@" | grep -E '^(Testing archive|Folders|Files|Size): |^(Method|Solid|Blocks) = |^Everything|^rc='; }
zs() { z "$@" | grep -vE '^(Archive size: |Physical Size = |Headers Size = )'; }
unhex() { printf "$(printf '%s' "$1" | sed 's/../\\x&/g')"; }
unhex 377abcaf271c0004018aa5cd1100000000000000d2000000000000008382ed4168656c6c6f0a776f726c6420776964650a010406000209060b00070b02000101000101000c060b00080a0120303a36500b6b07000005050e01e00f0120190f0000000000000000000000000000001149006400000064002f00730075006200000064002f0065006d00700074007900000064002f0061002e00740078007400000064002f007300750062002f0062002e00740078007400000019020000142a01000080e4d93156dd010080e4d93156dd010080e4d93156dd010080e4d93156dd010080e4d93156dd011516010010000000100000002000000020000000200000000000 > stored.7z
unhex 377abcaf271c00041ef126ade80300000000000023000000000000008a9e810ae05da1035d5d00341949ee8ddd16aa6184ab35eb7b32b25042b700604257da20e60b25064780c3b9229144c997bcb6ff9e3bf5f9714298c1ea8c9bdf1ddff040d3803816720cb1fa56253ed87e833e31deccbd255fa48bf544950f421cea330336a1514aceb78a66db8cbe6ed50b03cf11aa6db56a7ced9395cfb782f45928f3290900db8848bfdb982e72e0a0fbe0a818b25e16f1c488512bb5ca494b8aa2028d390edc0ae1896fd7e4d4368fab8c8f223bc9a8309243818694babb9f836cacf530d587a43832746ec10f4811a25aa722d7473f6db3565986c8b2838ba34ed19358b692f9a3cf60a3a8cea008bcb6f4951fe514542e0465b82fa749418525f88410691b26fd97f6ea4a70c3f8e0aef66039d814e666858e6a7b8ada10086f35d530c13a68e9567fdfa56ed5a7e4693ef64882f72d4e1d09fc10bd4b0edb009797512e55204674bd7e014ebfeec46976e5bfcfd869cbd91b3b108094f934bd2a5f024972d850646ebf48ee58e1457c3ba78fcce14259c01108a64cf720d43182d6360c5110f07d496cec76cb572e5b9498b947dd00f2beccf90af60519a828ae30847f35a409eeca373465b2791bc544988e3b8a882012bd705a4d1d2dc5df1a4b3e96bd64f7a1fb3de41495d7e26de64047d4361db956ffc796abad0a64a82cbee844cc3f10d84dc36ca18b5e1addd1ee31d7e4584bf2bcdd8a6f26a46d021aba5d87962b0ce4817544c37b06ab47154de08329695fa8282e6edafca85ebcfd0c9b9352d1addda5424d5979e58e4634156bdff68e3c2a394ceb931896e93cebf34e4f8d73552bb7be4f313371e9e46643e28ad560c2e2b7268de4880201ec86c224c89ebaa880267dc29a455aa1306b7b138c72ab54d94ff99274b6c9b740b4eaa875fea0f9b34149fe4eba4ced6794c8e5018d4daebcd2e0c22c0e6b502c5a43d54757e33032d20c30992d48c364b3eecf694fdb49dfbfc129373fa608879c96dd055b18390e479ebca649bf32b66295420da4be939bc9d28f6a12915b474468db239dccf7683b83349720ef32b2d7fe25def3b639beb88112b19e0f06ed00d8e66648731616ae64fa8e93c09a266af952615a658bcb835b52f82bc51fc8e1be9d7347dc4c571124578eb990665122ce4b68c59eb056e2ad02724107eac1b4034563d2522ffb463fdc53b61a07945517c3a22b2be812641498cb674aea1a8c2ac54600000000813307ae0fd5500fc65724d3feb37016b15cb7a822f6f50e51655529d6db2b689822ea26a0dcb111e501b92ddb741a878d5da042fa9322d8a305515bd001de1a34dc67bd926258cc005aece2204859abe02bc015b317e0fb282f833fd5f21a526cc6d4263e8eaecb95567106a98de35e50cd1c86d89b60a88978632b471fedf000170683650109808300070b01000123030101055d001000000c80ee0a010c29759f0000 > solid.7z
unhex 377abcaf271c0004e6f6765e10040000000000003f0000000000000060b0c6fa87393b2a6d119fff684ccddd97c5a9f3e2ee992f63c69b8f8c7dabdd94a08f0c14079005346b6109afd8ff0da11d66e54db6a992e4c0c1777f93079e18a5d1fc80b34afb767292743bd1fc65ae6367a7b167c8b301623ce40586a83587227752bdec3f938db8edcb5bea59ced589faf41a49b4a81884551c1b4632ea43e3124d33ef035d025eeacf76f20eb403d33cc442c85d55ccdda97a906e63ad1d9c272f60d3b0209df075b749fd84dda3874ca0fe925c1cf32bf0263f0ba0734a3e27347c7c4ab6c44e47b769544fa64ba7684d30ae973b8a0a011569ba8ee8185cc77a0f750b36a2ef5f39b28d65630f4c7dc710583f4f8a05c48ab55ca63033349b2b4ebf2ca78478822208ee18c80120f9723b27f736b9ff044c2a57d88857bb73da68579f8ede4d71e36995c7d93c9b96ca6bcab2e982ab44b61a4fa3849f18f71c6f6654a73bf6a23f22f4bdcc0e978011b06369e6ba1dbc753a2d7be7a2d59328a5e379ee92ea8d2059f357fc77cc235cec85a842948a742a33d99e2719c7a915b8cd98c8e74851634fa0f004a1b7df2382913bc4e7bffccb4c4cc24f6810ccdb741c59bbef3a53684ef2d2fea4a9d506aac3ade1f2e357ab82e4b76e8d139870878e953027daedb8d10dcc8fd4564abbf4777e0531e032f478c1d5e58e6b0b95c614afa5561401d055a746aed78726a8d049446c0024e3ca7e1039bbdada4335b860572dba7cc3e9825cbaadde33c3019b510d622da35361004badf88bc9802d106cf4ed3a42523824c624908a9a9d35206d9dee078e7595f125155e3a7e49b60f40662728af1b2d85640f2866bd382ca23d9ae3a61395177e6decb933925c6a1c2d29814e3a56c7b721727e1cc6c1d0b9067188896740592e19c69044d4fdad8ded7dcd46823cc8a20008f39b20b3ed6fc5ac44df7905aec1c722662aa327f6215ae363579a7b44b878d3fb60ba5d1196e683744acd1197d4f4d7b153aca04a374121adb738be8c336d5bb02c579a47dee50db19e8b6be5cff9e40958d636d4be05a384fe16fac08fb43360a22aec79a68fa9a27a98069903940f1e267ff2753926b07e0c825394202d44f9e9bd9ec0edd4dc90c761e9516af71520598789269c9d6594e5f220041b0262038961e3be06a1f842168d03f089a0c0cb0ed8984f84ecf39d0f298c3ea51589231ad9782195d7bcc877b98d6da96e0045df5150475dcef85561329274eea563a1942807b6b65185eb3f147854857676504cc0ec73faa388cf6185d2b2fb1eb0710f39d981310e12ac93d369d846663fdc8a861f35e68640dc05fde2ce070219aba4073b5ef0de37303364487892fc1875361438cc6a69ff424511845e3710da1258b4aede527844a9eda580a94788f418371d62cc3edc9b044a1f61d1dd1042ed2107c8f3a8b641f67338df576478e0c89f47850e4d20da4fafd04288ee52725a9c8f64f917068370010980a000070b0100022406f1070112530f175d129f716e50232d76ddbc91c0889723030101055d0010000001000c809f810e0a01aab52eb90000 > aes.7z

echo "== errors"
z
z l
z q solid.7z
z l -qq solid.7z
z l nosuch.7z
printf 'junk' > junk.7z; z l junk.7z
printf 'junk' > junk.dat; z l junk.dat
z l -tzip solid.7z
z l solid.7z nosuch.txt

echo "== list"
z l solid.7z
z l -slt solid.7z
z l -ba solid.7z
z l -ba -slt stored.7z
z l solid.7z d/a.txt
z l solid.7z -r '*.txt'
z l solid.7z '-x!text.txt'
z l stored.7z
z l '*.7z'

echo "== test"
z t solid.7z
z t stored.7z d/a.txt
z t -bb1 solid.7z
z t -bb3 stored.7z

echo "== passwords"
z l aes.7z
z l -pwrong aes.7z
z l -psecret aes.7z
z t -psecret aes.7z
z t -pwrong aes.7z

echo "== extract"
z x solid.7z -oout
find out | sort
cat out/d/a.txt out/d/sub/b.txt
z x solid.7z -oout
z x -y solid.7z -oout
z x -aos solid.7z -oout
z x -aou solid.7z -oout
find out | sort
z x -bb1 stored.7z -onew
z e stored.7z -oflat
find flat | sort
z x stored.7z -oone d/sub/b.txt
find one | sort
z x -so solid.7z d/a.txt
z e -so stored.7z
z x solid.7z -otwo nosuch.txt
find two 2>/dev/null | sort

echo "== damaged"
cp stored.7z bad.7z
printf 'H' | dd of=bad.7z bs=1 seek=32 conv=notrunc 2>/dev/null
z t bad.7z
z x bad.7z -obad
head -c 200 solid.7z > cut.7z
z t cut.7z

echo "== switches"
z l -yy solid.7z
z l -oa -ob solid.7z
z l -aox solid.7z
z l -bso0 nosuch.7z
z l -bse1 nosuch.7z
z t -bso2 stored.7z
z t -x stored.7z
z t '-xz!a' stored.7z
z -h
z t stored.7z -- -x

echo "== several"
mkdir many
cp solid.7z stored.7z many/
z l 'many/*.7z'
z t 'many/*.7z'
cp junk.7z many/
z t 'many/*.7z'

echo "== answers"
z x stored.7z -oans
printf 'n\nn\nn\n' | z x stored.7z -oans
printf 'y\nA\n' | z x stored.7z -oans
printf 's\n' | z x stored.7z -oans
printf 'x\nu\n' | z x stored.7z -oans
find ans | sort
printf 'q\n' | z x stored.7z -oans
z x -aot stored.7z -oans
find ans | sort

echo "== typed passwords"
printf 'secret\n' | z l aes.7z
printf 'secret\n' | z t aes.7z
printf 'wrong\n' | z t aes.7z

echo "== selection"
z t -r stored.7z '*.txt'
z l '-i!d/sub/*' solid.7z
z l '-ir!*.txt' solid.7z
z l '-x!d' solid.7z
z e -so stored.7z d/a.txt
z t -bb1 solid.7z d/sub/b.txt

echo "== tail"
cp stored.7z tail.7z
printf 'tail' >> tail.7z
z l tail.7z
z t tail.7z

echo "== here"
mkdir here
cd here
z x ../stored.7z
find . | sort
printf 'n\n' | z x ../stored.7z d/a.txt
cd ..

# The tree the update cases archive: what stored.7z holds, with its times.
mkdir tree
cd tree
z x ../stored.7z -bso0
printf 'twelve bytes' > d/c.txt
touch -d '2026-10-07 08:00:00' d/c.txt d d/sub
cd ..

echo "== add"
cd tree
z a -mx0 -mhc=off ../new.7z d
z l -slt ../new.7z
sha256sum ../new.7z | cut -c1-16
z a -mx0 -mhc=off -bb1 ../new.7z d/a.txt
z a -mx0 -mhc=off ../new.7z nosuch.txt
z a -mx0 -mhc=off ../empty.7z nosuch.txt
ls ../empty.7z 2>&1 | sed 's/.*: //'
z a -mx0 -mhc=off ../one.7z d/a.txt d/sub
z l -ba ../one.7z
z a -mx0 -mhc=off -r ../txt.7z '*.txt'
z l -ba ../txt.7z
z a -mx0 -mhc=off '-x!d/sub' ../nosub.7z d
z l -ba ../nosub.7z
cd ..

echo "== update"
cp new.7z up.7z
cd tree
printf 'changed\n' > d/a.txt
touch -d '2026-10-07 09:00:00' d/a.txt
touch -d '2026-10-07 08:00:00' d
z u -mx0 -mhc=off -bb1 ../up.7z d
z l -ba ../up.7z
sha256sum ../up.7z | cut -c1-16
cd ..

echo "== delete"
cp new.7z del.7z
z d -mhc=off del.7z d/a.txt
z l -ba del.7z
sha256sum del.7z | cut -c1-16
z d -mhc=off del.7z nosuch.txt
z d -mhc=off -bb1 del.7z d
z l -ba del.7z
z d nosuch.7z a

echo "== rename"
cp new.7z ren.7z
z rn -mhc=off ren.7z d/empty d/void d/sub d/folder
z l -ba ren.7z
sha256sum ren.7z | cut -c1-16
z rn -mhc=off ren.7z d/void

echo "== compressed"
cd tree
z a ../c.7z d -bso0
zl ../c.7z
zt ../c.7z
z a -mx1 -ms=off ../c1.7z d -bso0
zl ../c1.7z
z a -m0=PPMd -mhe=on -psecret ../cp.7z d -bso0
zl -psecret ../cp.7z
z a -mx9 -mf=off ../c9.7z d -bso0
zl ../c9.7z
z a -m0=BZip2 -mtc=on -mta=on ../cb.7z d/a.txt -bso0
zl ../cb.7z
cd ..

echo "== solid"
cp tree/../c.7z sol.7z
z d sol.7z d/c.txt -bso0
zl sol.7z
zt sol.7z
z rn sol.7z d/a.txt d/A.txt -bso0
zl sol.7z

echo "== verbose"
cp c.7z v.7z
zs d -bb2 v.7z d/c.txt
cd tree
zs u -bb3 ../v.7z d
cd ..
zl v.7z
cp new.7z v0.7z
z d -mhc=off -bb3 v0.7z d/c.txt

echo "== update errors"
cd tree
z a -mfoo=1 ../bad.7z d
z a -mfoo=1 ../bad2.7z d
ls ../bad2.7z 2>&1 | sed 's/.*: //'
z a ../junk.7z d/a.txt
z a -u- ../none.7z d/a.txt
ls ../none.7z 2>&1 | sed 's/.*: //'
z a -so ../so.7z d/a.txt
z a -uq9 ../q.7z d
z a -ur1 ../q.7z d
cd ..
z rn nosuch.7z a b
z rn missing.7z a b
z rn new.7z d/void
z rn new.7z 'd/*' x

echo "== switches for update"
cd tree
z a -mx0 -mhc=off -up0q0 ../sw.7z d/a.txt d/c.txt
z u -mx0 -mhc=off -uq0 ../sw.7z d/a.txt
z l -ba ../sw.7z
z a -mx0 -mhc=off '-u!../sw2.7z' ../sw.7z d/sub
z l -ba ../sw.7z
z l -ba ../sw2.7z
z a -mx0 -mhc=off -mtc=on -mta=on ../t.7z d/c.txt
z l -slt ../t.7z | grep -E '^(Path|Created|Accessed|Modified) =' | sed -E 's/^(Created|Accessed) = .+/\1 = (set)/'
z a -mx0 -mhc=off -mtm=off -mtr=off ../t2.7z d/c.txt
z l -slt ../t2.7z | grep -E '^(Path|Modified|Attributes) ='
z a -mx0 -mhc=off -r '-x!sub' ../x.7z '*.txt'
z l -ba ../x.7z
z a -mx0 -mhc=off ../nm d/c.txt
ls ../nm* | sed 's#.*/##'
cd ..
mkdir gone
printf 'one' > gone/1.txt
printf 'two' > gone/2.txt
z a -mx0 -mhc=off -sdel -bb1 gone.7z gone
ls gone/*.txt 2>/dev/null | wc -l
z l -ba gone.7z | cut -c20-

echo "== filters"
# An x64 PE head (BCJ), a 16-bit stereo PCM WAV (Delta:4), and text (no filter).
mkdir filt
z58=$(printf '%0116d' 0)
z14=$(printf '%028d' 0)
z936=$(printf '%01872d' 0)
unhex "4d5a${z58}40000000504500006486${z14}f00000000b02${z936}" > filt/app.exe
z256=$(printf '%0512d' 0)
unhex "524946462401000057415645666d7420100000000100020044ac000010b10200040010006461746100010000${z256}" > filt/s.wav
printf 'plain text\n' > filt/t.txt
touch -d '2026-10-07 08:00:00' filt/app.exe filt/s.wav filt/t.txt
cd filt
z a ../f.7z app.exe s.wav t.txt -bso0
zl ../f.7z
z a -mf=off ../f0.7z app.exe s.wav t.txt -bso0
zl ../f0.7z
z a -mf=BCJ ../f1.7z t.txt -bso0
zl ../f1.7z
z a -mqs -mf=off ../fq.7z t.txt app.exe s.wav -bso0
z l -ba ../fq.7z | cut -c54-
z a -ms=2f ../f2.7z app.exe s.wav t.txt -mf=off -bso0
zl ../f2.7z | grep -E '^(Path|Block|Blocks) '
printf 'pw\n' | z a -p -mx0 ../pw.7z t.txt
z l -slt -ppw ../pw.7z | grep -E '^(Path|Encrypted|Method) = '
cd ..

echo "== named formats"
printf 'n\n' > n1.txt
printf 'm\n' > n2.txt
touch -d '2026-10-07 08:00:00' n1.txt n2.txt
z a -tzip -mx0 -bso0 nz.zip n1.txt
cp nz.zip nz.7z
z a nz.7z n2.txt
zs a -mm=PPMd mm.7z n1.txt -bso0
zl mm.7z
zs a -mhc- -mtc- -mx0 hc.7z n1.txt n2.txt
od -A n -t x1 -N 8 hc.7z

echo "== hash"
mkdir -p hd/d/sub
cd hd || exit 1
printf 'alpha\n' > d/a.txt
printf 'beta\n' > d/sub/b.txt
: > d/empty
touch -d '2026-10-07 08:00:00' d/a.txt d/sub/b.txt d/empty d/sub d
z h d
z h d/a.txt
z h -scrcsha256 d/a.txt d/sub
z h -scrcCRC32 -scrcCRC64 -scrcSHA1 d/a.txt
z h '-scrc*' d/a.txt
z h -ba d
z h -scrcfoo d/a.txt
z h nothere
z h d/empty
z h -r '*.txt'
printf 'alpha\n' | z h -si
z h -scrcBLAKE2sp d/a.txt
z h -scrcXXH64 -scrcMD5 -scrcSHA512 -scrcSHA384 -scrcSHA3-256 d/a.txt
z h -bb3 d/sub
z a -bso0 h.7z d
z t -scrcSHA256 h.7z
z x -scrcSHA256 -ohx h.7z
z t -scrc h.7z
z e -scrc -so h.7z d/a.txt
z t -scrcfoo h.7z
cd ..

echo "== volumes"
mkdir -p vd/d
cd vd || exit 1
seq 1 3000 > d/n.txt
printf 'alpha\n' > d/a.txt
touch -d '2026-10-07 08:00:00' d/n.txt d/a.txt d
sizes() { for f in "$@"; do echo "$(wc -c < "$f") $f"; done; }
z a -mx0 -mhc=off -v5k x.7z d
sizes x.7z.*
cksum < x.7z.001
cat x.7z.001 x.7z.002 x.7z.003 | cksum
z a -mx0 -mhc=off -v5k -v3k y.7z d
sizes y.7z.*
z a -mx0 -mhc=off -v5000b w.7z d
sizes w.7z.*
z a -mx0 -mhc=off -v100k one.7z d
ls one.7z*
z l x.7z.001
z l -slt x.7z.001
z t x.7z.001
z x -ovx x.7z.001
ls vx/d
z l x.7z.002
z l 'x.7z.*'
mv x.7z.003 x.7z.3
z t x.7z.001
mv x.7z.3 x.7z.003
z a -mx0 -mtm- -v5k x.zip d
sizes x.zip.*
cat x.zip.001 x.zip.002 x.zip.003 | cksum
z l x.zip.001
z a -v5k x.tar d
sizes x.tar.*
cat x.tar.0* | cksum
z t x.tar.001
z a -bso0 -v2k x.gz d/n.txt
z t x.gz.001 | grep -E '^(Type|Everything is Ok|rc)'
z u -v5k x.7z d
z a -v0 bad.7z d
z a -vx bad.7z d
7z a -so -v5k -ttar so d > /dev/null
echo "rc=$?"
cd ..
