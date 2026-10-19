# source: https://github.com/asdf-vm/asdf/tree/v0.15.0 (its `bin`, `lib`, `help.txt`, `version.txt` and `defaults`, unchanged, as the fixture directory `asdf`; the last release in bash)
# desc: a version manager of many sourced files, offline: a plugin made in place, its versions listed and the latest found, two installed and one refused, a global version written, project and local versions found in .tool-versions files up the tree, current, where and which, a command run through asdf exec and through its shim, and an uninstall
# timeout: 60
# asdf looks for .asdfrc and .tool-versions from $PWD up until it reaches `/`, which in
# cash, where $PWD is C:/..., it never does (the differences page); both are here, at the
# top, so that every search ends before the root.
export ASDF_DIR="$PWD/asdf" ASDF_DATA_DIR="$PWD/data" HOME="$PWD/home"
mkdir -p home work "$ASDF_DATA_DIR/plugins/fake/bin"
: > .asdfrc
echo "fake 1.1.0" > .tool-versions
p="$ASDF_DATA_DIR/plugins/fake/bin"
printf '%s\n' '#!/usr/bin/env bash' 'echo "1.0.0 1.1.0 2.0.0"' > "$p/list-all"
printf '%s\n' '#!/usr/bin/env bash' \
  'case "$ASDF_INSTALL_VERSION" in 1.0.0 | 1.1.0 | 2.0.0) ;; *) echo "fake: no version $ASDF_INSTALL_VERSION" >&2; exit 1 ;; esac' \
  'mkdir -p "$ASDF_INSTALL_PATH/bin"' \
  'printf "%s\n" "#!/usr/bin/env bash" "echo \"fake-tool $ASDF_INSTALL_VERSION: \$*\"" > "$ASDF_INSTALL_PATH/bin/fake-tool"' \
  'chmod +x "$ASDF_INSTALL_PATH/bin/fake-tool"' > "$p/install"
chmod +x "$p/list-all" "$p/install"
asdf() { bash "$ASDF_DIR/bin/asdf" "$@"; }
top=$PWD
current() { asdf current fake | sed "s|$top|TOP|"; }
asdf version
asdf plugin list
asdf list all fake
asdf latest fake
asdf install fake 1.1.0; echo "install: $?"
asdf install fake 2.0.0; echo "install: $?"
asdf install fake 9.9.9; echo "install: $?"
asdf list fake
asdf global fake 2.0.0; cat home/.tool-versions
current
cd work || exit 1
current
asdf local fake 2.0.0; cat .tool-versions
current
w=$(asdf where fake); echo "where: ${w#"$ASDF_DATA_DIR"/}"
w=$(asdf which fake-tool); echo "which: ${w#"$ASDF_DATA_DIR"/}"
asdf exec fake-tool one two
ls "$ASDF_DATA_DIR/shims"
bash "$ASDF_DATA_DIR/shims/fake-tool" through the shim
cd .. || exit 1
bash "$ASDF_DATA_DIR/shims/fake-tool" from the top
asdf uninstall fake 2.0.0; echo "uninstall: $?"
asdf list fake
cd work && current; echo "current: ${PIPESTATUS[0]}"
