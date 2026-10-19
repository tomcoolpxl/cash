# source: https://github.com/TekWizely/bash-tpl/blob/v0.9.1/bash-tpl (the fixture `bash-tpl`, unchanged)
# desc: a real template engine: a template turned into a script and that script run, text tags, quote tags, printf-format tags, statement lines and blocks with their indentation kept, an include, changed delimiters, comments kept and dropped, a template from standard input
printf '%s\n' \
  '%# dropped from the script' \
  '% # kept in the script' \
  '%' \
  '    count=0' \
  '%' \
  'Names:' \
  '% for n in "$@"; do' \
  '    % count=$((count + 1))' \
  '    - <% $n %> (<%" $n "%>)' \
  '    .INCLUDE item.tpl' \
  '% done' \
  'Count: <%| %03d | $count %>' \
  'Shout: <%% echo "${1:-none}" | tr a-z A-Z %>' \
  '.DELIMS tag="{{ }}"' \
  'Braces: {{ $# }} names' > page.tpl
printf '%s\n' '  * item <% $count %>' > item.tpl
bash ./bash-tpl page.tpl > page.sh; echo "made: $?"
grep -c 'kept in the script' page.sh
grep -c 'dropped from' page.sh
bash page.sh ann bob
bash page.sh
printf '%s\n' 'Piped: <% $1 %>' | bash ./bash-tpl > piped.sh
bash piped.sh in
bash ./bash-tpl --version
bash ./bash-tpl nosuch.tpl 2>/dev/null; echo "missing: $?"
