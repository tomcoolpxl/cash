# 7z on RAR 5 and 7 archives, run under 7-Zip 26.03 (the oracle: Scoop's 7z.exe on
# Windows) and under cash's builtin: rars' fixtures (crates/cash-archive/tests/fixtures/rar),
# the archives of WinRAR 7.12 and 7.21, libarchive's and rarfile's tests, and rars' own,
# listed technically and in columns, tested, and some extracted; volume sets from their
# first volume and a later one; encrypted headers with the password, a wrong one and none;
# and files named as RAR volumes that are none. 7z_rar.out is 7-Zip's output.
#
# As in 7z_cases.sh, `z` turns 7-Zip's CRLF and `\` into LF and `/`. 7-Zip shows times in
# Windows' zone by the bias of the day it runs (`FileTimeToLocalFileTime`), whatever the
# time's own date; cash here runs in UTC, so the golden file's times are taken back to UTC
# by that bias.
#
# Regenerate the golden file (on Windows, with Scoop's 7-Zip and a cash without the
# builtin, such as 1.8.0), `b` being the day's bias in seconds (7200 in Brussels' summer,
# 3600 in its winter):
#   cash 7z_rar.sh | TZ=UTC0 awk -v b=7200 '{ s = $0; o = "";
#     while (match(s, /[0-9]{4}-[0-9][0-9]-[0-9][0-9] [0-9][0-9]:[0-9][0-9]:[0-9][0-9]/)) {
#       t = substr(s, RSTART, RLENGTH); gsub(/[-:]/, " ", t);
#       o = o substr(s, 1, RSTART - 1) strftime("%Y-%m-%d %H:%M:%S", mktime(t) - b, 1);
#       s = substr(s, RSTART + RLENGTH) }
#     print o s }' > 7z_rar.out

exec 2>&1 </dev/null
export TZ=UTC0
fx=$(cd ../../../cash-archive/tests/fixtures/rar && pwd)
dir=$(mktemp -d)
cd "$dir" || exit 1
z() { 7z "$@" > "$dir/o.txt" 2>&1; r=$?; tr -d '\r' < "$dir/o.txt" | sed 's#\\#/#g'; echo "rc=$r"; }
cp -r "$fx/golden" "$fx/rar50" .

for a in golden/stored_comment.rar golden/stored_comment_metadata.rar \
    golden/stored_quick_open.rar golden/stored_rar50.rar golden/stored_rar70.rar \
    golden/stored_recovery_1.rar golden/stored_recovery_10.rar golden/stored_recovery_5.rar \
    golden/stored_recovery_50.rar golden/stored_volume_0.rar golden/stored_volume_1.rar \
    golden/stored_volume_2.rar golden/stored_volume_3.rar \
    rar50/algorithm_version_2.rar rar50/algorithm_version_2_stored.rar \
    rar50/ams_archive_name_rar721.rar rar50/crc32_wrong_beside_blake2sp.rar \
    rar50/empty_file.rar rar50/encrypted_multivol.part1.rar rar50/encrypted_multivol.part2.rar \
    rar50/encrypted_multivol.part3.rar rar50/filter_arm.rar rar50/filter_delta.rar \
    rar50/filter_e8.rar rar50/filter_e8e9.rar rar50/first_block_without_tables.rar \
    rar50/m1_fastest.rar rar50/m3_default.rar rar50/m5_max.rar rar50/multifile.rar \
    rar50/multivol.part1.rar rar50/multivol.part2.rar rar50/multivol.part3.rar \
    rar50/multivol_rev.part1.rar rar50/multivol_rev.part2.rar rar50/multivol_rev.part3.rar \
    rar50/multivol_rev.part4.rar rar50/multivol_rev.part5.rar rar50/password_aes.rar \
    rar50/password_crc32.rar rar50/plaintext_stored_multivol.part2.rar rar50/solid.rar \
    rar50/solid_multivol.part01.rar rar50/solid_multivol.part02.rar \
    rar50/solid_multivol.part03.rar rar50/solid_multivol.part04.rar \
    rar50/solid_multivol.part05.rar rar50/solid_multivol.part06.rar rar50/stored.rar \
    rar50/stored_blake2.rar rar50/stored_multivol.part1.rar rar50/stored_multivol.part2.rar \
    rar50/stored_multivol.part3.rar rar50/subdata_size_underflow.rar rar50/wild/hardlink.rar \
    rar50/wild/invalid_hash_valid_htime_exfld.rar rar50/wild/libarchive_loop_bug.rar \
    rar50/wild/libarchive_multiple_files_solid.rar rar50/wild/libarchive_solid.rar \
    rar50/wild/rarfile_hlink.rar rar50/wild/rarfile_solid.rar rar50/wild/rarfile_solid_qo.rar \
    rar50/wild/symlink.rar rar50/with_all_services.rar rar50/with_comment.rar \
    rar50/with_quickopen.rar rar50/with_recovery.rar rar50/zero_fill_out_of_window.rar \
    rar50/zeroed_password_check.rar; do
  echo "== l -slt $a"
  z l -slt "$a"
done

for a in rar50/multivol.part1.rar rar50/multivol.part2.rar rar50/solid_multivol.part01.rar \
    rar50/plaintext_stored_multivol.part2.rar golden/stored_volume_0.rar rar50/with_comment.rar; do
  echo "== l $a"
  z l "$a"
done
echo "== l, a set's volumes named together: the later ones are read with the first"
z l 'rar50/multivol.part*.rar'

echo "== encrypted headers, with the password"
z l -slt -ppassword rar50/header_encrypted.rar
z l -ppassword rar50/header_encrypted_comment.rar
z l -ppassword rar50/header_encrypted_stored_multivol.part1.rar
z l -slt -pPassword rar50/winrar721_header_encrypted_quickopen.rar
echo "== a wrong password"
z l -pwrong rar50/header_encrypted.rar
echo "== none, and none to type"
z l rar50/header_encrypted.rar

printf 'not an archive\n' > x.rar
cp x.rar x.r00
cp x.rar x.r01
cp x.rar x.part1.rar
for a in x.rar x.r00 x.r01 x.part1.rar; do
  echo "== l $a, which is not one"
  z l "$a"
done


# t: each archive above, with the password where one is wanted; the out-of-window
# match left out, which 7-Zip's decoder refuses and rar's fills with zeros, as WinRAR
# does. x: a selection, each file written and its CRC; symbolic links left out, as
# making one needs a right a CI runner has and a desktop may not.
for a in golden/stored_comment.rar golden/stored_comment_metadata.rar \
    golden/stored_quick_open.rar golden/stored_rar50.rar golden/stored_rar70.rar \
    golden/stored_recovery_1.rar golden/stored_recovery_10.rar \
    golden/stored_recovery_5.rar golden/stored_recovery_50.rar \
    golden/stored_volume_0.rar golden/stored_volume_1.rar \
    golden/stored_volume_2.rar golden/stored_volume_3.rar \
    rar50/algorithm_version_2.rar rar50/algorithm_version_2_stored.rar \
    rar50/ams_archive_name_rar721.rar rar50/crc32_wrong_beside_blake2sp.rar \
    rar50/empty_file.rar rar50/encrypted_multivol.part1.rar \
    rar50/encrypted_multivol.part2.rar rar50/encrypted_multivol.part3.rar \
    rar50/filter_arm.rar rar50/filter_delta.rar rar50/filter_e8.rar \
    rar50/filter_e8e9.rar rar50/first_block_without_tables.rar rar50/m1_fastest.rar \
    rar50/m3_default.rar rar50/m5_max.rar rar50/multifile.rar \
    rar50/multivol.part1.rar rar50/multivol.part2.rar rar50/multivol.part3.rar \
    rar50/multivol_rev.part1.rar rar50/multivol_rev.part2.rar \
    rar50/multivol_rev.part3.rar rar50/multivol_rev.part4.rar \
    rar50/multivol_rev.part5.rar rar50/password_aes.rar rar50/password_crc32.rar \
    rar50/plaintext_stored_multivol.part2.rar rar50/solid.rar \
    rar50/solid_multivol.part01.rar rar50/solid_multivol.part02.rar \
    rar50/solid_multivol.part03.rar rar50/solid_multivol.part04.rar \
    rar50/solid_multivol.part05.rar rar50/solid_multivol.part06.rar \
    rar50/stored.rar rar50/stored_blake2.rar rar50/stored_multivol.part1.rar \
    rar50/stored_multivol.part2.rar rar50/stored_multivol.part3.rar \
    rar50/subdata_size_underflow.rar rar50/wild/hardlink.rar \
    rar50/wild/invalid_hash_valid_htime_exfld.rar \
    rar50/wild/libarchive_loop_bug.rar \
    rar50/wild/libarchive_multiple_files_solid.rar rar50/wild/libarchive_solid.rar \
    rar50/wild/rarfile_hlink.rar rar50/wild/rarfile_solid.rar \
    rar50/wild/rarfile_solid_qo.rar rar50/wild/symlink.rar \
    rar50/with_all_services.rar rar50/with_comment.rar rar50/with_quickopen.rar \
    rar50/with_recovery.rar rar50/header_encrypted.rar \
    rar50/header_encrypted_comment.rar \
    rar50/header_encrypted_stored_multivol.part1.rar; do
  echo "== t -ppassword $a"
  z t -ppassword "$a"
done
for a in rar50/zeroed_password_check.rar; do
  echo "== t -psecret $a"
  z t -psecret "$a"
done
for a in rar50/winrar721_header_encrypted_quickopen.rar; do
  echo "== t -pPassword $a"
  z t -pPassword "$a"
done

# `x` into a folder of its own, then each file it holds and its CRC.
xt() {
  rm -rf out
  z x -y -oout "$@"
  (cd out 2>/dev/null && find . -type f | sort | while read -r f; do echo "$f $(cksum < "$f")"; done)
}
echo "== x rar50/multifile.rar"
xt rar50/multifile.rar
echo "== x rar50/solid.rar"
xt rar50/solid.rar
echo "== x rar50/m5_max.rar"
xt rar50/m5_max.rar
echo "== x rar50/multivol.part1.rar"
xt rar50/multivol.part1.rar
echo "== x rar50/solid_multivol.part01.rar"
xt rar50/solid_multivol.part01.rar
echo "== x rar50/stored_multivol.part1.rar"
xt rar50/stored_multivol.part1.rar
echo "== x rar50/filter_arm.rar"
xt rar50/filter_arm.rar
echo "== x rar50/filter_delta.rar"
xt rar50/filter_delta.rar
echo "== x rar50/filter_e8.rar"
xt rar50/filter_e8.rar
echo "== x rar50/filter_e8e9.rar"
xt rar50/filter_e8e9.rar
echo "== x rar50/algorithm_version_2.rar"
xt rar50/algorithm_version_2.rar
echo "== x rar50/with_all_services.rar"
xt rar50/with_all_services.rar
echo "== x golden/stored_rar70.rar"
xt golden/stored_rar70.rar
echo "== x rar50/wild/hardlink.rar"
xt rar50/wild/hardlink.rar
echo "== x rar50/wild/rarfile_hlink.rar"
xt rar50/wild/rarfile_hlink.rar
echo "== x rar50/wild/libarchive_solid.rar"
xt rar50/wild/libarchive_solid.rar
echo "== x rar50/wild/libarchive_multiple_files_solid.rar"
xt rar50/wild/libarchive_multiple_files_solid.rar
echo "== x -ppassword rar50/password_aes.rar"
xt -ppassword rar50/password_aes.rar
echo "== x -ppassword rar50/password_crc32.rar"
xt -ppassword rar50/password_crc32.rar
echo "== x -ppassword rar50/encrypted_multivol.part1.rar"
xt -ppassword rar50/encrypted_multivol.part1.rar
echo "== x -ppassword rar50/header_encrypted.rar"
xt -ppassword rar50/header_encrypted.rar
echo "== x rar50/solid.rar tiny.txt"
xt rar50/solid.rar tiny.txt
echo "== e, a wrong password"
rm -rf out
z e -y -oout -pwrong rar50/password_aes.rar
echo "== a: RAR is not written, to a new archive named so nor to one there"
printf 'x\n' > f.txt
z a new.rar f.txt
z a rar50/stored.rar f.txt
ls new.rar 2>&1 | wc -l

# WinRAR 7.23's -oi:1 (see rar50/README-copy-links.md): s/d.bin is a copy link to s/b.bin.
echo "== copy links: one to a file that starts its stream decodes that file again, one to a"
echo "== solid file takes what was kept as that file was decoded"
for a in rar50/copy_links_plain.rar rar50/copy_links_solid.rar; do
  echo "== x -bb3 $a"
  xt -bb3 "$a"
  echo "== t -bb3 $a"
  z t -bb3 "$a"
  echo "== t -scrcCRC32 $a"
  z t -scrcCRC32 "$a"
  echo "== x -bb3 $a s/d.bin"
  xt -bb3 "$a" s/d.bin
  echo "== t -bb3 $a s/d.bin"
  z t -bb3 "$a" s/d.bin
done
echo "== a link to a solid file that no stream after goes on from: 7-Zip does not decode the"
echo "== file, and writes the link empty"
xt -bb3 rar50/copy_links_groups.rar s/d.bin
z t -bb3 rar50/copy_links_groups.rar s/d.bin
echo "== a link inside a solid stream decodes its file into the stream, and the file after"
echo "== fails; a link only skipped is not decoded"
z t rar50/copy_links_mid_stream.rar
xt -bb3 rar50/copy_links_mid_stream.rar t/f.bin
echo "== a link to an encrypted file asks for the password"
xt -ppassword rar50/copy_links_password.rar s/d.bin
xt rar50/copy_links_password.rar s/d.bin

# Damaged RAR 5 data, one byte of an archive set to U (0x55): what decoded before the
# fault is written, a filter 7-Zip cannot take is an unsupported method, and a solid
# stream goes on after a damaged file.
dmg() { { head -c "$2" "$1"; printf 'U'; tail -c +$(($2 + 2)) "$1"; } > dmg.rar; }
echo "== a link decoded inside a solid stream: the file after it fails, half written"
xt rar50/copy_links_mid_stream.rar
for case in m3_default:1090 m3_default:1284 solid_text:1535 solid_text:4487 filter_e8:120 \
    filter_e8:135; do
  echo "== damaged: rar50/${case%:*}.rar at ${case#*:}"
  dmg "rar50/${case%:*}.rar" "${case#*:}"
  xt dmg.rar
done

echo "== alternate streams: listed with -sns, counted apart, extracted into their file"
echo "== unless -sns-, and with their file"
z l rar50/alt_streams.rar
z l -sns rar50/alt_streams.rar
z t rar50/alt_streams.rar
xt rar50/alt_streams.rar
xt -sns- rar50/alt_streams.rar
xt rar50/alt_streams.rar s/a.txt
echo "== ACLs: a file's NT Security, as 7-Zip shows a descriptor"
z l -slt rar50/nt_security_crafted.rar | grep '^Path = f\|^Characteristics\|^NT Security'

echo "== an archive after other bytes, as after an SFX's module, found by its signature:"
echo "== named .exe and no PE, a warning; named .rar, not looked for"
{ printf 'MZ%01022d' 0; cat rar50/multifile.rar; } > sfx.exe
z l sfx.exe
xt sfx.exe
cp sfx.exe sfx.dat
z t sfx.dat
cp sfx.exe sfx.rar
z l sfx.rar

cd / && rm -rf "$dir"
