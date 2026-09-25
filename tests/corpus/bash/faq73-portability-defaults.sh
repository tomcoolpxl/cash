# source: https://mywiki.wooledge.org/BashFAQ/073 (Portability)
# desc: ${var-word} ${var+word} ${var=word} ${var:-word} on unset / empty / set
for state in unset empty set; do
  unset var
  case $state in empty) var= ;; set) var=val ;; esac
  printf '%s: [%s] [%s] [%s] [%s] ' "$state" "${var-word}" "${var+word}" "${var:-word}" "${var:+word}"
  : "${var=assigned}"; printf '[%s]\n' "$var"
done
( unset var; echo "${var?custom error}" ) 2>/dev/null; echo "status=$?"
