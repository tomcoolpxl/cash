# source: https://github.com/kvz/bash3boilerplate/tree/v2.8.0 (`main.sh`, `example.sh` and `src` but `megamount.sh`, unchanged, with its LICENSE, as the fixture directory `bash3boilerplate`)
# desc: a template for robust scripts under set -o errexit, nounset and pipefail: options parsed from the usage text in a here-document, repeated and long ones with `=`, a required one refused, log levels to standard error, a trap on exit; and its helpers parse_url, ini_val and templater, as functions and as commands; the times and paths it logs are left out
# timeout: 30
b=bash3boilerplate
strip() { sed -e 's/^[0-9-]* [0-9:]* UTC //' -e '/__dir:\|__file:\|__base:\|OSTYPE:/d'; }
printf 'data\n' > in.txt
bash $b/main.sh --help 2>&1 | strip; echo "status: ${PIPESTATUS[0]}"
bash $b/main.sh -n -f in.txt -1 2>&1 | strip; echo "status: ${PIPESTATUS[0]}"
bash $b/main.sh -n --file=in.txt -i a -i b -x -x -x 2>&1 | strip; echo "status: ${PIPESTATUS[0]}"
bash $b/main.sh -n 2>&1 | strip; echo "status: ${PIPESTATUS[0]}"
LOG_LEVEL=3 bash $b/main.sh -n -f in.txt 2>&1 | strip; echo "status: ${PIPESTATUS[0]}"
bash $b/main.sh -n -f in.txt -t 2>/dev/null | strip; echo "status: ${PIPESTATUS[0]}"
bash $b/example.sh -n -f in.txt 2>&1 | strip; echo "status: ${PIPESTATUS[0]}"
echo "== parse_url"
(
  source $b/src/parse_url.sh
  for part in proto user pass host port path; do
    echo "$part: $(parse_url 'https://johndoe:abc123@example.com:8443/a/b.html' $part)"
  done
  parse_url 'ftp://example.org/pub'
)
bash $b/src/parse_url.sh 'http://example.com:8080/index.html'
echo "== ini_val"
printf '%s\n' '[connection]' 'host = 10.0.0.1' 'port = 5432' '' '[software]' 'name = cash' > data.ini
(
  source $b/src/ini_val.sh
  echo "host: $(ini_val data.ini connection.host)"
  ini_val data.ini connection.host 127.0.0.1 "Host name or IP address"
  ini_val data.ini software.version 1.35
  ini_val data.ini orphan nowhere
)
cat data.ini
bash $b/src/ini_val.sh data.ini software.name
echo "== templater"
printf '%s\n' 'name = ${NAME}' 'port = ${PORT}' 'kept = ${MISSING}' > in.cfg
NAME=kevin PORT=8080 bash $b/src/templater.sh in.cfg out.cfg 2>&1; echo "status: $?"
NAME=kevin PORT=8080 ALLOW_REMAINDERS=1 bash $b/src/templater.sh in.cfg out.cfg 2>&1; echo "status: $?"
cat out.cfg
