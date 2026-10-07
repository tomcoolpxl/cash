# rar's and unrar's listings, run under WinRAR 7.23's Rar.exe and UnRAR.exe (the oracle:
# Scoop's extras/winrar on Windows) and under cash's builtins: rars' fixtures
# (crates/cash-archive/tests/fixtures/rar), RAR 1.3 to 7, each listed technically with
# its service headers (`lta`) and verbosely (`v`), some in every other form; volume sets
# from their first volume and a later one, alone and with `-v`; encrypted headers with
# the password, a wrong one and none; files named as RAR archives that are none.
# rar_list.out is the originals' output.
#
# rars' own RAR 4 archives are left out: their MS-DOS times are 0, which rar shows as
# whatever its memory holds, a different time each listing.
#
# `z` keeps standard output and standard error apart and turns CRLF and `\` into LF and
# `/`. rar shows times in Windows' zone, each by its own date's rules, whatever `TZ`
# says; cash shows them in `TZ`'s, so the script names Brussels', where the golden file
# was made.
#
# Regenerate the golden file on Windows, with Scoop's WinRAR and a cash without the
# builtins, such as 1.10.0, in Brussels' zone:
#   cash rar_list.sh > rar_list.out

exec </dev/null
export TZ=Europe/Brussels
# rar writes names and comments into files in Windows' ANSI code page unless told
# UTF-8, which cash writes everywhere.
export RARINISWITCHES=-scfr
fx=$(cd ../../../cash-archive/tests/fixtures/rar && pwd)
dir=$(mktemp -d)
cd "$dir" || exit 1
z() {
  "$@" > "$dir/o.txt" 2> "$dir/e.txt"
  r=$?
  tr -d '\r' < "$dir/o.txt" | sed 's#\\#/#g'
  if [ -s "$dir/e.txt" ]; then
    echo "--- stderr"
    tr -d '\r' < "$dir/e.txt" | sed 's#\\#/#g'
    echo
  fi
  echo "rc=$r"
}
cp -r "$fx/golden" "$fx/rar50" "$fx/rar15_40" "$fx/rar13" .

for a in golden/stored_comment.rar golden/stored_comment_metadata.rar \
    golden/stored_quick_open.rar golden/stored_rar50.rar golden/stored_rar70.rar \
    golden/stored_recovery_1.rar golden/stored_recovery_10.rar \
    golden/stored_recovery_5.rar golden/stored_recovery_50.rar \
    golden/stored_volume_0.rar golden/stored_volume_1.rar golden/stored_volume_2.rar \
    golden/stored_volume_3.rar \
    rar50/algorithm_version_2.rar rar50/algorithm_version_2_stored.rar \
    rar50/ams_archive_name_rar721.rar rar50/crc32_wrong_beside_blake2sp.rar \
    rar50/empty_file.rar rar50/encrypted_multivol.part1.rar \
    rar50/encrypted_multivol.part2.rar rar50/encrypted_multivol.part3.rar \
    rar50/filter_arm.rar rar50/filter_delta.rar rar50/filter_e8.rar \
    rar50/filter_e8e9.rar rar50/first_block_without_tables.rar rar50/m1_fastest.rar \
    rar50/m3_default.rar rar50/m5_max.rar rar50/multifile.rar \
    rar50/multivol.part1.rar rar50/multivol.part2.rar rar50/multivol.part3.rar \
    rar50/multivol_rev.part1.rar rar50/multivol_rev.part3.rar \
    rar50/multivol_rev.part5.rar rar50/password_aes.rar rar50/password_crc32.rar \
    rar50/plaintext_stored_multivol.part2.rar rar50/solid.rar \
    rar50/solid_multivol.part01.rar rar50/solid_multivol.part03.rar \
    rar50/solid_multivol.part06.rar rar50/stored.rar rar50/stored_blake2.rar \
    rar50/stored_multivol.part1.rar rar50/stored_multivol.part2.rar \
    rar50/subdata_size_underflow.rar rar50/wild/hardlink.rar \
    rar50/wild/invalid_hash_valid_htime_exfld.rar rar50/wild/libarchive_loop_bug.rar \
    rar50/wild/libarchive_multiple_files_solid.rar rar50/wild/libarchive_solid.rar \
    rar50/wild/rarfile_hlink.rar rar50/wild/rarfile_solid.rar \
    rar50/wild/rarfile_solid_qo.rar rar50/wild/symlink.rar rar50/with_all_services.rar \
    rar50/with_comment.rar rar50/with_quickopen.rar rar50/with_recovery.rar \
    rar50/zero_fill_out_of_window.rar rar50/zeroed_password_check.rar \
    rar15_40/empty_compressed_payload_rar30.rar \
    rar15_40/encrypted/per_file_rar300_password.rar \
    rar15_40/encrypted/per_file_rar4_libarchive_mixed.rar \
    rar15_40/encrypted/rar4_junrar_file_content_encrypted_unicode.rar \
    rar15_40/encrypted/rar4_junrar_password.rar \
    rar15_40/encrypted/rar4_mixed_visible_names_password.rar \
    rar15_40/encrypted/rar4_sharpcompress_files_only.rar \
    rar15_40/node_unrar_js/with_comment.rar rar15_40/ppmd/farmanager170.rar \
    rar15_40/ppmd/ppmd_escape_rar300.rar rar15_40/ppmd/ppmd_lz_repeat_rar3.cbr \
    rar15_40/ppmd/ppmd_solid_rar300.rar rar15_40/rar154/audio_dos_names_unpack15.rar \
    rar15_40/rar154/audio_win_names_unpack15.rar rar15_40/rar154/doc_154_best.rar \
    rar15_40/rar154/random.rar rar15_40/rar154/random.r01 \
    rar15_40/rar154/readme_154_password.rar rar15_40/rar154/readme_154_store_solid.rar \
    rar15_40/rar202/comment_nopsw.rar rar15_40/rar202/comment_psw.rar \
    rar15_40/rar250/AUDIO.RAR rar15_40/rar250/SOLID.RAR \
    rar15_40/rar250/unpack20_multiblock.rar rar15_40/rar250_protect_head_rr1.rar \
    rar15_40/rar250_protect_head_rr5.rar \
    rar15_40/rar300/compressed_multivol_prng_rar300.rar \
    rar15_40/rar300/compressed_multivol_prng_rar300.r01 \
    rar15_40/rar300/compressed_text_rar300.rar \
    rar15_40/rar300/encrypted_multivol_rar300.rar \
    rar15_40/rar300/encrypted_newnaming_rar300.part02.rar \
    rar15_40/rar300/multivol_newnaming_rar300.part01.rar \
    rar15_40/rar300/multivol_oldnaming_rar300.r00 \
    rar15_40/rar300/rarvm_x86_e8_rar300.rar rar15_40/rar300/rev_newstyle.part1.rar \
    rar15_40/rar300/rev_oldstyle.part3.rar rar15_40/rar300/solid_rar300.rar \
    rar15_40/rar300/stored_multivol_rar300.rar rar15_40/rar300/with_comment_rar300.rar \
    rar15_40/rar300/with_compressed_recovery_header_synthetic.rar \
    rar15_40/rar300/with_compressed_recovery_rar300.rar \
    rar15_40/rar300/with_recovery_rar300.rar rar15_40/rar420/ext_time_rar420.rar \
    rar15_40/rarvm/delta_64_channels.rar \
    rar15_40/rarvm/generic_delta_padding_mutation.rar rar15_40/solid_flag_cleared_rar15.rar \
    rar15_40/zero_fill/rar20.rar rar15_40/zero_head_size.rar \
    rar13/BIG80K.RAR rar13/CMULTIV.RAR rar13/CMULTIV.R03 rar13/COMMENT.RAR \
    rar13/EMPTY.RAR rar13/FCOMM.RAR rar13/MULTIFIL.RAR rar13/MULTIVOL.RAR \
    rar13/README.RAR rar13/README_password=password.rar rar13/README_store.rar \
    rar13/REPEATB.RAR rar13/SOLID.RAR rar13/STOREPWD.RAR rar13/WITHDIR.RAR \
    rar13/encrypted_split/ESPLIT.RAR rar13/rar140_av/rar140_av_patched.rar \
    rar13/rar140_av/rar140_noav_baseline.rar rar13/solid_flag_cleared.rar; do
  echo "== lta $a"
  z rar lta "$a"
  echo "== v $a"
  z rar v "$a"
done

echo "== each form, on archives with comments, services, links and volumes"
for a in rar50/multifile.rar rar50/with_comment.rar rar50/with_all_services.rar \
    rar50/wild/symlink.rar rar50/wild/hardlink.rar rar50/multivol.part2.rar \
    rar15_40/rar300/with_comment_rar300.rar rar13/MULTIFIL.RAR \
    golden/stored_comment_metadata.rar; do
  for c in l lt lb vt vta vb; do
    echo "== $c $a"
    z rar $c "$a"
  done
done

echo "== volumes listed with -v, from the first and from a later one"
for a in rar50/multivol.part1.rar rar50/multivol.part2.rar rar50/solid_multivol.part01.rar \
    rar15_40/rar300/multivol_oldnaming_rar300.rar \
    rar15_40/rar300/compressed_multivol_prng_rar300.r01 rar13/CMULTIV.RAR; do
  echo "== l -v $a"
  z rar l -v "$a"
  echo "== vt -v $a"
  z rar vt -v "$a"
done
for a in rar50/multivol.part1.rar rar15_40/rar300/multivol_newnaming_rar300.part01.rar \
    rar15_40/rar300/compressed_multivol_prng_rar300.rar; do
  echo "== v -v $a"
  z rar v -v "$a"
  echo "== lb -v $a"
  z rar lb -v "$a"
done

echo "== encrypted headers, with the password"
for a in rar50/header_encrypted.rar rar50/header_encrypted_comment.rar \
    rar50/header_encrypted_stored_multivol.part1.rar \
    rar15_40/encrypted/header_rar300_password.rar \
    rar15_40/encrypted/header_rar420_password.rar \
    rar15_40/encrypted/rar4_junrar_header_encrypted.rar \
    rar15_40/rar300/header_encrypted_multivol_rar300.rar \
    rar15_40/rar300/header_encrypted_newnaming_rar300.part01.rar; do
  echo "== lta -ppassword $a"
  z rar lta -ppassword "$a"
done
z rar lta -p1234 rar15_40/encrypted/header_enc_1234.rar
z rar v -pPassword rar50/winrar721_header_encrypted_quickopen.rar
echo "== a wrong password"
z rar l -pwrong rar50/header_encrypted.rar
z rar l -pwrong rar15_40/encrypted/header_rar300_password.rar
echo "== none, and none to type"
z rar l rar50/header_encrypted.rar
z rar l rar15_40/encrypted/header_rar300_password.rar

echo "== files that are no archives"
printf 'not an archive\n' > x.rar
cp x.rar x.r00
cp x.rar x.part1.rar
for a in x.rar x.r00 x.part1.rar missing.rar; do
  echo "== l $a"
  z rar l "$a"
done

echo "== several archives, by a wildcard and by name"
z rar lb 'rar50/multi*'
z rar l rar50/stored.rar rar50/solid.rar

echo "== unrar"
for a in rar50/multifile.rar rar50/with_comment.rar rar15_40/rar300/solid_rar300.rar \
    rar13/MULTIFIL.RAR; do
  for c in l lta v vb; do
    echo "== unrar $c $a"
    z unrar $c "$a"
  done
done
z unrar l -ppassword rar50/header_encrypted.rar
z unrar l x.rar

cd / && rm -rf "$dir"
