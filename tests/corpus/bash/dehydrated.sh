# source: https://github.com/dehydrated-io/dehydrated/tree/v0.7.2 (`dehydrated`, unchanged, with its LICENSE, as the fixture directory `dehydrated`)
# desc: an ACME client of one bash script, run where no CA is reached: its help built from its own comments, its configuration read from a file and printed with typeset -p, unused certificate files archived and deleted, and, sourced with DEHYDRATED_NOOP, its JSON parser and getters, reverse DNS names, hex conversions and domains.txt reader; openssl a stand-in
# timeout: 60
# Git Bash turns hexdump's `/1 "%02x"` into a Windows path without this; cash reads it not.
export MSYS_NO_PATHCONV=1
mkdir -p wellknown
printf '#!/bin/sh\necho "OpenSSL 3.0.0 (stand-in)"\n' > fake-openssl
chmod +x fake-openssl
cat > config <<'EOF'
CA="https://acme.example.invalid/directory"
CERTDIR="${BASEDIR}/certs"
DOMAINS_TXT="${BASEDIR}/domains.txt"
OPENSSL="${BASEDIR}/fake-openssl"
WELLKNOWN="${BASEDIR}/wellknown"
CONTACT_EMAIL="me@example.com"
KEYSIZE="2048"
RENEW_DAYS="30"
EOF
p() { sed -e "s|$PWD|PWD|g"; }
bash dehydrated/dehydrated --help | p
echo "help: $?"
bash dehydrated/dehydrated --env 2>&1 | p
echo "env: $?"
mkdir -p certs/example.com certs/example.org
for f in cert-1.pem cert-2.pem chain-1.pem privkey-1.pem fullchain-2.pem cert-1.csr; do
    echo x > "certs/example.com/$f"
done
echo x > certs/example.org/cert-3.pem
bash dehydrated/dehydrated --cleanup 2>&1 | p
echo "cleanup: $?"
find archive certs | sort
echo x > certs/example.org/privkey-4.pem
bash dehydrated/dehydrated --cleanup-delete 2>&1 | p
echo "cleanup-delete: $?"
find certs | sort
printf '# domains\nExample.COM www.example.com\n\n  api.example.org   >  api-alias\nTEST.example.net>other\n' > domains.txt
bash -c '
DEHYDRATED_NOOP=NOOP
. dehydrated/dehydrated
set +e
json='"'"'{"status": "valid", "expires": "2026-12-01T00:00:00Z",
  "identifier": {"type": "dns", "value": "example.com"},
  "challenges": [{"type": "http-01", "token": "abc"}, {"type": "dns-01", "token": "def"}],
  "wildcard": false, "retry": 3}'"'"'
printf "%s\n" "$json" | clean_json
printf "%s" "$json" | clean_json | jsonsh
printf "%s" "$json" | clean_json | jsonsh | get_json_string_value status
printf "%s" "$json" | clean_json | jsonsh | get_json_int_value retry
printf "%s" "$json" | clean_json | jsonsh | get_json_bool_value wildcard
printf "%s" "$json" | clean_json | jsonsh | get_json_dict_value identifier
printf "%s" "$json" | clean_json | jsonsh | get_json_string_value -p "\"challenges\",1,\"token\""
json_path status; echo; json_path -p "\"a\",0"; echo
printf "192.168.1.20" | ip_to_ptr; echo
printf "2001:db8::1" | ip_to_ptr; echo
printf "48656c6c6f" | hex2bin; echo
printf "Hi!" | bin2hex; echo
DOMAINS_TXT=domains.txt parse_domains_txt
printf "[1,2" | jsonsh; echo "bad json: $?"
' 2>&1 | p
