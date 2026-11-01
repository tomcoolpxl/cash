# source: https://github.com/tfutils/tfenv/tree/de6ce2e (`bin`, `lib`, `libexec`, `test/test_common.sh`, `test/test_commands.sh` and LICENSE, unchanged, as the fixture directory `tfenv`)
# desc: tfenv at de6ce2e (3.2.2), the Terraform version manager: its commands suite (help, --version, no arguments, an unknown command), and, with three versions installed by hand as stand-ins that print their version, list, use, version-name, pin, the terraform shim, use latest, a .terraform-version with a CRLF and with comments, version-file, resolve-version, uninstall and the errors of a version that is not there; the work directory's path is hidden in both spellings; the remote is refused, so nothing is downloaded, and install, list-remote and the suites that need them are left out
# timeout: 300
# unixpaths: tfenv-version-file walks up the directory tree with `${root%/*}` until the
# root is empty, which a Windows-spelled path never is (jenv had the same); Bash has no
# such option, and says so on standard error.
shopt -s unixpaths 2> /dev/null
here=$PWD; windows=$(pwd -W 2> /dev/null)
hide() { sed -e "s|$here|WORK|g" -e "s|$windows|WORK|g"; }
export HOME="$PWD/home"; mkdir -p "$HOME"
export TFENV_ROOT="$PWD/tfenv" TFENV_CONFIG_DIR="$PWD/config" TFENV_AUTO_INSTALL=false TFENV_REMOTE=http://127.0.0.1:9/
export PATH="$TFENV_ROOT/bin:$PATH"
echo "== the commands suite"
bash tfenv/test/test_commands.sh 2>&1 | hide
echo "== three versions installed by hand"
for v in 1.6.1 1.0.11 0.14.6; do
  mkdir -p "config/versions/$v"
  printf '%s\n' '#!/usr/bin/env bash' "echo \"Terraform v$v\"" > "config/versions/$v/terraform"
  chmod +x "config/versions/$v/terraform"
done
mkdir -p work && cd work || exit 1
tfenv list 2>&1 | hide
echo "== use 1.0.11, pin, the shim"
tfenv use 1.0.11 2>&1 | hide
tfenv version-name
tfenv pin 2>&1 | hide
cat .terraform-version
terraform --version
echo "== use latest"
rm .terraform-version
tfenv use latest 2>&1 | hide
tfenv version-name
echo "== a version file with a CRLF, then with comments"
printf '1.6.1\r\n' > .terraform-version
tfenv version-name
printf '# pinned\n\n0.14.6 # old\n' > .terraform-version
tfenv version-name
tfenv version-file | hide
echo "== resolve-version"
tfenv resolve-version 1.6.1
tfenv resolve-version latest:^1.0
tfenv resolve-version latest
echo "== uninstall"
tfenv uninstall 0.14.6 2>&1 | hide
tfenv version-name 2>&1 | hide; echo "status: $?"
rm .terraform-version
tfenv list 2>&1 | hide
echo "== errors"
tfenv use 9.9.9 2>&1 | hide; echo "status: ${PIPESTATUS[0]}"
tfenv uninstall 9.9.9 2>&1 | hide; echo "status: ${PIPESTATUS[0]}"
tfenv pin extra 2>&1 | hide; echo "status: ${PIPESTATUS[0]}"
