# 7z on RAR 1.5 to 4 archives, run under 7-Zip 26.03 (the oracle: Scoop's 7z.exe on
# Windows) and under cash's builtin: rars' fixtures (crates/cash-archive/tests/fixtures/rar,
# made by WinRAR 1.54 to 4.20, libarchive's, junrar's, node-unrar-js's and rars' own),
# listed technically and in columns; volume sets of both namings from their first volume
# and a later one; encrypted headers with the password, a wrong one and none. Four with
# names outside ASCII are left out: 7-Zip writes those in the console's code page, cash in
# UTF-8. 7z_rar4.out is 7-Zip's output.
#
# As in 7z_cases.sh, `z` turns 7-Zip's CRLF and `\` into LF and `/`. RAR 1.5 to 4 keep
# MS-DOS's local times, which 7-Zip shows as they are kept, whatever the zone, and so does
# cash in UTC.
#
# Regenerate the golden file on Windows, with Scoop's 7-Zip and a cash without the
# builtin, such as 1.8.0:
#   cash 7z_rar4.sh > 7z_rar4.out

exec 2>&1 </dev/null
export TZ=UTC0
fx=$(cd ../../../cash-archive/tests/fixtures/rar && pwd)
dir=$(mktemp -d)
cd "$dir" || exit 1
z() { 7z "$@" > "$dir/o.txt" 2>&1; r=$?; tr -d '\r' < "$dir/o.txt" | sed 's#\\#/#g'; echo "rc=$r"; }
cp -r "$fx/rar15_40" .

for a in rar15_40/empty_compressed_payload_rar30.rar \
    rar15_40/encrypted/per_file_rar300_password.rar \
    rar15_40/encrypted/per_file_rar4_libarchive_mixed.rar \
    rar15_40/encrypted/rar4_junrar_password.rar rar15_40/ppmd/farmanager170.rar \
    rar15_40/ppmd/ppmd_escape_rar300.rar rar15_40/ppmd/ppmd_lorem_rar300.rar \
    rar15_40/ppmd/ppmd_lz_match_rar300.rar rar15_40/ppmd/ppmd_lz_repeat_rar3.cbr \
    rar15_40/ppmd/ppmd_mixed_rar300.rar rar15_40/ppmd/ppmd_solid_rar300.rar \
    rar15_40/rar154/audio_dos_names_unpack15.rar \
    rar15_40/rar154/audio_win_names_unpack15.rar rar15_40/rar154/doc_154_best.rar \
    rar15_40/rar154/random.r00 rar15_40/rar154/random.r01 rar15_40/rar154/random.rar \
    rar15_40/rar154/readme_154_normal.rar rar15_40/rar154/readme_154_password.rar \
    rar15_40/rar154/readme_154_store_solid.rar rar15_40/rar202/comment_nopsw.rar \
    rar15_40/rar202/comment_psw.rar rar15_40/rar250/AUDIO.RAR \
    rar15_40/rar250/AUTOREJ.RAR rar15_40/rar250/BIGLZ.RAR rar15_40/rar250/SOLID.RAR \
    rar15_40/rar250/unpack20_audio_text.rar rar15_40/rar250/unpack20_keep_tables.rar \
    rar15_40/rar250/unpack20_multiblock.rar rar15_40/rar250_protect_head_rr1.rar \
    rar15_40/rar250_protect_head_rr5.rar \
    rar15_40/rar300/compressed_multivol_prng_rar300.r00 \
    rar15_40/rar300/compressed_multivol_prng_rar300.r01 \
    rar15_40/rar300/compressed_multivol_prng_rar300.r02 \
    rar15_40/rar300/compressed_multivol_prng_rar300.r03 \
    rar15_40/rar300/compressed_multivol_prng_rar300.rar \
    rar15_40/rar300/compressed_text_rar300.rar \
    rar15_40/rar300/encrypted_multivol_rar300.r00 \
    rar15_40/rar300/encrypted_multivol_rar300.rar \
    rar15_40/rar300/encrypted_newnaming_rar300.part01.rar \
    rar15_40/rar300/encrypted_newnaming_rar300.part02.rar \
    rar15_40/rar300/multivol_newnaming_rar300.part01.rar \
    rar15_40/rar300/multivol_newnaming_rar300.part02.rar \
    rar15_40/rar300/multivol_oldnaming_rar300.r00 \
    rar15_40/rar300/multivol_oldnaming_rar300.rar \
    rar15_40/rar300/rarvm_audio_stereo_rar300.rar \
    rar15_40/rar300/rarvm_delta_4ch_rar300.rar \
    rar15_40/rar300/rarvm_itanium_synthetic_rar300.rar \
    rar15_40/rar300/rarvm_rgb_gradient_rar300.rar \
    rar15_40/rar300/rarvm_x86_e8_rar300.rar rar15_40/rar300/rarvm_x86_e8e9_rar300.rar \
    rar15_40/rar300/rev_newstyle.part1.rar rar15_40/rar300/rev_newstyle.part2.rar \
    rar15_40/rar300/rev_newstyle.part3.rar rar15_40/rar300/rev_newstyle.part4.rar \
    rar15_40/rar300/rev_oldstyle.part1.rar rar15_40/rar300/rev_oldstyle.part2.rar \
    rar15_40/rar300/rev_oldstyle.part3.rar rar15_40/rar300/rev_oldstyle.part4.rar \
    rar15_40/rar300/solid_rar300.rar rar15_40/rar300/solid_simple_rar300.rar \
    rar15_40/rar300/stored_multivol_rar300.r00 \
    rar15_40/rar300/stored_multivol_rar300.r01 \
    rar15_40/rar300/stored_multivol_rar300.r02 \
    rar15_40/rar300/stored_multivol_rar300.rar rar15_40/rar300/with_comment_rar300.rar \
    rar15_40/rar300/with_compressed_recovery_header_synthetic.rar \
    rar15_40/rar300/with_compressed_recovery_rar300.rar \
    rar15_40/rar300/with_recovery_rar300.rar rar15_40/rar420/ext_time_rar420.rar \
    rar15_40/rars_generated/comments.rar rar15_40/rars_generated/compressed.rar \
    rar15_40/rars_generated/encrypted.rar rar15_40/rars_generated/solid.rar \
    rar15_40/rars_generated/split-encrypted.r00 \
    rar15_40/rars_generated/split-encrypted.r01 \
    rar15_40/rars_generated/split-encrypted.rar rar15_40/rars_generated/split-store.r00 \
    rar15_40/rars_generated/split-store.r01 rar15_40/rars_generated/split-store.rar \
    rar15_40/rars_generated/stored.rar rar15_40/rarvm/delta_64_channels.rar \
    rar15_40/rarvm/filter_bsdcat_exe.rar \
    rar15_40/rarvm/generic_delta_padding_mutation.rar \
    rar15_40/rarvm/ppmd_embedded_vm_filter.rar \
    rar15_40/rarvm/solid_e8_filter_member_offset.rar \
    rar15_40/rarvm/vm_encoded_u32_filter.rar rar15_40/solid_flag_cleared_rar15.rar \
    rar15_40/zero_fill/rar20.rar rar15_40/zero_fill/rar29.rar \
    rar15_40/zero_head_size.rar; do
  echo "== l -slt $a"
  z l -slt "$a"
done

for a in rar15_40/rar300/multivol_oldnaming_rar300.rar \
    rar15_40/rar300/multivol_oldnaming_rar300.r00 \
    rar15_40/rar300/multivol_newnaming_rar300.part01.rar \
    rar15_40/rar300/multivol_newnaming_rar300.part02.rar \
    rar15_40/rar300/compressed_multivol_prng_rar300.rar; do
  echo "== l $a"
  z l "$a"
done

echo "== encrypted headers, with the password"
z l -slt -p1234 rar15_40/encrypted/header_enc_1234.rar
z l -slt -ppassword rar15_40/encrypted/header_rar300_password.rar
z l -slt -ppassword rar15_40/encrypted/header_rar420_password.rar
z l -slt -pjunrar rar15_40/encrypted/rar4_junrar_header_encrypted.rar
z l -slt -ppassword rar15_40/rar300/header_encrypted_multivol_rar300.rar
z l -slt -ppassword rar15_40/rar300/header_encrypted_newnaming_rar300.part01.rar
echo "== a wrong password"
z l -pwrong rar15_40/encrypted/header_rar300_password.rar
echo "== none, and none to type"
z l rar15_40/encrypted/header_rar300_password.rar

cd / && rm -rf "$dir"
