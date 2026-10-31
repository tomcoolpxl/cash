# source: https://github.com/jenv/jenv/tree/a771525 (`libexec`, `completions`, `available-plugins` and LICENSE, unchanged, as the fixture directory `jenv`; `bin/jenv`, a link to `libexec/jenv` upstream, is left out and `libexec` put on PATH)
# desc: jenv, the Java version manager (its own suite wants the JDKs of a Docker image): two JDKs made by hand with a stand-in java added, their aliases read from `java -version`, the init script for bash loaded, versions listed, the global, local and shell versions set and read back, the shims, which, whence, prefix, javahome and exec, the export plugin and JAVA_HOME through it, a removal refused (a version is a link upstream, a copy on Windows, in both shells), the commands and plugins listed, doctor and the version; the directory's path is hidden
# timeout: 300
# unixpaths: jenv walks a directory up with `${root%/*}` until it is empty, which a
# Windows-spelled `C:/…` never is, as rbenv's family does; Bash has no such option, and
# says so on standard error.
shopt -s unixpaths 2> /dev/null
export HOME="$PWD/home"; mkdir -p "$HOME"
export JENV_ROOT="$HOME/.jenv"
export PATH="$PWD/jenv/libexec:$PATH"
# The machine's Java, if any, stays out of it.
unset JAVA_HOME JDK_HOME
here=$PWD; windows="${here:1:1}"; windows="${windows^^}:${here:2}"
hide() { sed -e "s|$here|WORK|g" -e "s|$windows|WORK|g"; }
# A JDK is a directory with a `java` that answers `-version` as the real one does, on
# standard error, and says what it was asked otherwise.
jdk() {
  mkdir -p "jdks/$1/bin"
  printf '%s\n' '#!/bin/sh' 'if [ "$1" = -version ]; then' "  printf '%s\\n' '$2' '$3' '$4' >&2" 'else' "  echo \"java $1 \$*\"" 'fi' > "jdks/$1/bin/java"
  chmod +x "jdks/$1/bin/java"
}
jdk temurin17 'openjdk version "17.0.2" 2022-01-18' 'OpenJDK Runtime Environment Temurin-17.0.2+8 (build 17.0.2+8)' 'OpenJDK 64-Bit Server VM Temurin-17.0.2+8 (build 17.0.2+8, mixed mode, sharing)'
jdk oracle8 'java version "1.8.0_392"' 'Java(TM) SE Runtime Environment (build 1.8.0_392-b08)' 'Java HotSpot(TM) 64-Bit Server VM (build 25.392-b08, mixed mode)'
main() {
  echo "== add"; jenv add "$PWD/jdks/temurin17"; jenv add "$PWD/jdks/oracle8"
  echo "== add by alias"; jenv add custom "$PWD/jdks/temurin17"
  echo "== not a jdk"; jenv add "$PWD/jdks"
  # The shell is named: left to find it, jenv asks `ps -p $PPID -o args=`, which neither
  # shell's ps answers, and falls back on `$SHELL`, cash's own name here.
  echo "== init"; eval "$(jenv init - bash)"; echo "shell: $JENV_SHELL loaded: $JENV_LOADED"; echo "PATH starts at ${PATH%%:*}"
  echo "== versions"; jenv versions
  echo "== global"; jenv global 17.0; jenv version; jenv version-name; jenv version-origin; cat "$JENV_ROOT/version"
  echo "== which, whence, prefix, javahome"; jenv which java; jenv whence java; jenv prefix; jenv javahome
  echo "== shims"; jenv shims; java -version 2>&1; java -jar app.jar
  echo "== local"; mkdir -p project && cd project && jenv local 1.8 && cat .java-version && jenv version && java -version 2>&1; jenv local --unset; jenv version; cd ..
  echo "== shell"; jenv shell temurin64-17.0.2; jenv version; echo "JENV_VERSION=$JENV_VERSION"; jenv shell --unset; jenv version
  echo "== exec"; jenv exec java -version 2>&1; JENV_VERSION=1.8 jenv exec java --help
  echo "== export plugin"; jenv enable-plugin export 2>&1; jenv plugins | sed -n '1,4p'; jenv refresh-plugins; eval "$(jenv init - bash)"; echo "JAVA_HOME=$JAVA_HOME"
  echo "== commands"; jenv commands | head -8; jenv commands --sh
  # A version and a plugin are symbolic links upstream, which Windows gives Git Bash as
  # a copy and cash as a directory link, so adding a version again, removing one and
  # doctor's view of the plugin's link differ between the shells and are left out.
  echo "== doctor"; jenv doctor 2>&1 | grep -v 'Plugin export is linked\|Please execute'
  echo "== version"; jenv --version
}
main 2>&1 | hide
