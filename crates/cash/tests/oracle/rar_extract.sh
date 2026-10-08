# rar's and unrar's t, x, e and p, run under WinRAR 7.23's Rar.exe and UnRAR.exe (the
# oracle: Scoop's extras/winrar on Windows) and under cash's builtins: rars' fixtures
# (crates/cash-archive/tests/fixtures/rar), RAR 1.3 to 7, tested; some extracted, each
# file written and its CRC; files that are there, by each -o and by answers piped in;
# the path switches, masks, passwords, damaged data, volumes from the first and a later
# one. rar_extract.out is the originals' output.
#
# As in rar_list.sh, `z` keeps standard output and standard error apart and turns CRLF
# and `\` into LF and `/`; it drops the percentages rar writes, with backspaces, even
# into a file, and the backspaces, as many as either writes. Symbolic links are left out of what is extracted: making one needs a
# right a CI runner has and a desktop may not. rar reads answers from a pipe a buffer at
# a time, the whole of a short one at its first question, and so does cash. Masks
# take `\`: Windows' rar matches none with `/`, where cash takes both.
#
# Regenerate the golden file on Windows, with Scoop's WinRAR and a cash without the
# builtins, such as 1.10.0, in Brussels' zone:
#   cash rar_extract.sh > rar_extract.out

exec </dev/null
export TZ=Europe/Brussels
# rar writes names into files in Windows' ANSI code page unless told UTF-8.
export RARINISWITCHES=-scfr
fx=$(cd ../../../cash-archive/tests/fixtures/rar && pwd)
dir=$(mktemp -d)
cd "$dir" || exit 1
z() {
  "$@" > "$dir/o.txt" 2> "$dir/e.txt"
  r=$?
  tr -d '\r' < "$dir/o.txt" | sed -e 's/\x08\{1,\}[ 0-9]\{3\}%//g' -e 's/\x08//g' -e 's#\\#/#g'
  if [ -s "$dir/e.txt" ]; then
    echo "--- stderr"
    tr -d '\r' < "$dir/e.txt" | sed -e 's/\x08\{1,\}[ 0-9]\{3\}%//g' -e 's/\x08//g' -e 's#\\#/#g'
    echo
  fi
  echo "rc=$r"
}
# What a folder holds: each file and its CRC, each folder.
tree() {
  (cd "$1" 2>/dev/null && find . -mindepth 1 | sort | while read -r f; do
    if [ -d "$f" ]; then echo "$f/"; else echo "$f $(cksum < "$f")"; fi
  done)
}
cp -r "$fx/golden" "$fx/rar50" "$fx/rar15_40" "$fx/rar13" .

for a in golden/stored_comment.rar golden/stored_quick_open.rar golden/stored_rar70.rar \
    golden/stored_recovery_5.rar golden/stored_volume_0.rar \
    rar50/algorithm_version_2.rar rar50/algorithm_version_2_stored.rar \
    rar50/ams_archive_name_rar721.rar rar50/crc32_wrong_beside_blake2sp.rar \
    rar50/empty_file.rar rar50/filter_arm.rar rar50/filter_delta.rar rar50/filter_e8.rar \
    rar50/filter_e8e9.rar rar50/first_block_without_tables.rar rar50/m1_fastest.rar \
    rar50/m3_default.rar rar50/m5_max.rar rar50/multifile.rar rar50/multivol.part1.rar \
    rar50/multivol.part2.rar rar50/multivol.part3.rar rar50/multivol_rev.part1.rar \
    rar50/solid.rar rar50/solid_multivol.part01.rar rar50/solid_multivol.part04.rar \
    rar50/stored.rar rar50/stored_blake2.rar rar50/stored_multivol.part1.rar \
    rar50/subdata_size_underflow.rar rar50/wild/hardlink.rar \
    rar50/wild/invalid_hash_valid_htime_exfld.rar rar50/wild/libarchive_loop_bug.rar \
    rar50/wild/libarchive_multiple_files_solid.rar rar50/wild/libarchive_solid.rar \
    rar50/wild/rarfile_hlink.rar rar50/wild/rarfile_solid.rar \
    rar50/wild/rarfile_solid_qo.rar rar50/wild/symlink.rar rar50/with_all_services.rar \
    rar50/with_comment.rar rar50/with_quickopen.rar rar50/with_recovery.rar \
    rar50/zero_fill_out_of_window.rar \
    rar15_40/empty_compressed_payload_rar30.rar rar15_40/ppmd/farmanager170.rar \
    rar15_40/ppmd/ppmd_escape_rar300.rar rar15_40/ppmd/ppmd_lz_match_rar300.rar \
    rar15_40/ppmd/ppmd_solid_rar300.rar rar15_40/rar154/audio_dos_names_unpack15.rar \
    rar15_40/rar154/doc_154_best.rar rar15_40/rar154/random.rar \
    rar15_40/rar154/random.r00 rar15_40/rar154/readme_154_store_solid.rar \
    rar15_40/rar202/comment_nopsw.rar rar15_40/rar250/AUDIO.RAR \
    rar15_40/rar250/AUTOREJ.RAR rar15_40/rar250/BIGLZ.RAR rar15_40/rar250/SOLID.RAR \
    rar15_40/rar250/unpack20_audio_text.rar rar15_40/rar250/unpack20_keep_tables.rar \
    rar15_40/rar250/unpack20_multiblock.rar rar15_40/rar250_protect_head_rr1.rar \
    rar15_40/rar300/compressed_multivol_prng_rar300.rar \
    rar15_40/rar300/compressed_multivol_prng_rar300.r02 \
    rar15_40/rar300/compressed_text_rar300.rar \
    rar15_40/rar300/multivol_newnaming_rar300.part01.rar \
    rar15_40/rar300/multivol_oldnaming_rar300.rar \
    rar15_40/rar300/rarvm_audio_stereo_rar300.rar \
    rar15_40/rar300/rarvm_delta_4ch_rar300.rar \
    rar15_40/rar300/rarvm_itanium_synthetic_rar300.rar \
    rar15_40/rar300/rarvm_rgb_gradient_rar300.rar rar15_40/rar300/rarvm_x86_e8_rar300.rar \
    rar15_40/rar300/rarvm_x86_e8e9_rar300.rar rar15_40/rar300/rev_newstyle.part1.rar \
    rar15_40/rar300/solid_rar300.rar rar15_40/rar300/solid_simple_rar300.rar \
    rar15_40/rar300/stored_multivol_rar300.rar rar15_40/rar300/with_comment_rar300.rar \
    rar15_40/rar300/with_recovery_rar300.rar rar15_40/rar420/ext_time_rar420.rar \
    rar15_40/rarvm/delta_64_channels.rar rar15_40/rarvm/filter_bsdcat_exe.rar \
    rar15_40/rarvm/generic_delta_padding_mutation.rar \
    rar15_40/rarvm/solid_e8_filter_member_offset.rar \
    rar15_40/rarvm/vm_encoded_u32_filter.rar rar15_40/solid_flag_cleared_rar15.rar \
    rar15_40/zero_fill/rar20.rar rar15_40/zero_fill/rar29.rar \
    rar13/BIG80K.RAR rar13/CMULTIV.RAR rar13/COMMENT.RAR rar13/EMPTY.RAR rar13/FCOMM.RAR \
    rar13/MULTIFIL.RAR rar13/MULTIVOL.RAR rar13/README.RAR rar13/README_store.rar \
    rar13/REPEATB.RAR rar13/SOLID.RAR rar13/WITHDIR.RAR rar13/solid_flag_cleared.rar; do
  echo "== t $a"
  z rar t "$a"
done

echo "== t, encrypted, with the password, a wrong one and none"
for a in rar50/password_aes.rar rar50/password_crc32.rar rar50/encrypted_multivol.part1.rar \
    rar50/header_encrypted.rar rar15_40/encrypted/per_file_rar300_password.rar \
    rar15_40/encrypted/per_file_rar4_libarchive_mixed.rar \
    rar15_40/encrypted/header_rar300_password.rar rar15_40/rar154/readme_154_password.rar \
    "rar13/README_password=password.rar"; do
  echo "== t -ppassword $a"
  z rar t -ppassword "$a"
  echo "== t -pwrong $a"
  z rar t -pwrong "$a"
  echo "== t $a"
  z rar t "$a"
done
z rar t -psecret rar50/zeroed_password_check.rar
z rar t -p1234 rar15_40/encrypted/header_enc_1234.rar
echo "== -mes skips what is encrypted"
z rar t -mes rar15_40/encrypted/per_file_rar4_libarchive_mixed.rar

echo "== t, the messages' switches"
for s in -idn -idd -idp -idq -idc -inul -ierr -c-; do
  echo "== t $s"
  z rar t $s rar50/with_comment.rar
done

echo "== t, masks"
z rar t rar50/multifile.rar hello.txt
z rar t rar50/multifile.rar '*.bin' tiny.txt
z rar t rar50/multifile.rar nomatch
z rar t -x'*.txt' rar50/multifile.rar
z rar t -n'*.txt' rar50/multifile.rar
z rar t rar13/WITHDIR.RAR SUBDIR
z rar t rar13/WITHDIR.RAR 'subdir\*'

echo "== t, damaged data"
cp rar50/stored.rar bad5.rar
printf 'X' | dd of=bad5.rar bs=1 seek=100 conv=notrunc 2>/dev/null
cp rar15_40/rar300/compressed_text_rar300.rar bad4.rar
printf 'XXXX' | dd of=bad4.rar bs=1 seek=120 conv=notrunc 2>/dev/null
z rar t bad5.rar
z rar t bad4.rar
mkdir lonely && cp rar50/multivol.part1.rar lonely/
z rar t lonely/multivol.part1.rar

echo "== x into a new folder, then again over what is there"
z rar x rar50/multifile.rar out1/
tree out1
z rar x rar50/multifile.rar out1/
z rar x -o+ rar50/multifile.rar out1/
z rar x -y rar50/multifile.rar out1/
z rar x -o- rar50/multifile.rar out1/
z rar x -or rar50/multifile.rar out1/
tree out1
echo "== answers piped in: y, n, A, E, Q"
for answer in y n a e q Y N; do
  rm -rf out2 && mkdir out2 && cp out1/hello.txt out1/tiny.txt out2/
  echo "== answer $answer"
  printf '%s\n' "$answer" | z rar x rar50/multifile.rar out2/
  tree out2
done
echo "== a rename, its name piped in"
rm -rf out2 && mkdir out2 && cp out1/hello.txt out2/
printf 'r\nrenamed.txt\n' | z rar x rar50/multifile.rar out2/
tree out2

echo "== the path switches"
z rar x rar13/WITHDIR.RAR out3/
tree out3
z rar e rar13/WITHDIR.RAR out4/
tree out4
z rar x -ep rar13/WITHDIR.RAR out5/
tree out5
z rar x -ep1 rar13/WITHDIR.RAR 'SUBDIR\*' out6/
tree out6
z rar x -apSUBDIR rar13/WITHDIR.RAR out7/
tree out7
z rar x -ep4SUBDIR rar13/WITHDIR.RAR out8/
tree out8
z rar x -ad rar50/multifile.rar out9/
tree out9
z rar x -opout10 rar50/multifile.rar
tree out10
z rar x -opout11/deeper rar13/WITHDIR.RAR
tree out11
z rar x -x'*.bin' rar50/multifile.rar out12/
tree out12
z rar x -cl rar13/WITHDIR.RAR out13/
tree out13

echo "== volumes, from the first and from a later one"
z rar x rar50/multivol.part1.rar out14/
tree out14
z rar x rar50/solid_multivol.part03.rar out15/
tree out15
z rar x rar15_40/rar300/multivol_oldnaming_rar300.r00 out16/
tree out16
z rar x lonely/multivol.part1.rar out17/
tree out17

echo "== encrypted, extracted"
z rar x -ppassword rar50/password_aes.rar out18/
tree out18
z rar x -pwrong rar50/password_aes.rar out19/
tree out19
z rar x -ppassword rar50/header_encrypted.rar out20/
tree out20

echo "== damaged, extracted, kept or not"
z rar x bad5.rar out21/
tree out21
z rar x -kb bad5.rar out22/
tree out22

echo "== p"
z rar p rar50/multifile.rar hello.txt
z rar p rar50/multifile.rar tiny.txt hello.txt
z rar p -inul rar13/MULTIFIL.RAR TINY.TXT
z rar p -ppassword rar50/password_aes.rar
z rar p rar50/password_aes.rar
z rar p bad5.rar

echo "== unrar"
z unrar t rar50/multifile.rar
z unrar x rar50/multifile.rar out23/
tree out23
z unrar e rar13/WITHDIR.RAR out24/
tree out24
z unrar p rar50/multifile.rar hello.txt
unrar a new.rar out23 > /dev/null 2>&1
echo "unrar a: rc=$?"

cd / && rm -rf "$dir"
