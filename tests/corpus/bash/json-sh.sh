# source: https://github.com/dominictarr/JSON.sh/blob/master/JSON.sh (the fixture `JSON.sh`, unchanged)
# desc: a real sh script sourced: egrep -o tokens through a read loop, local in functions, every option, and its errors
. ./JSON.sh
json() { ( parse_options "$@"; tokenize | parse ); }
doc='{"a":1,"b":[true,false,null],"c":{"d":"x\"y","e":-1.5e3},"f":"","g":[],"h":{}}'
printf '%s' "$doc" | json
echo "-- leaf only"
printf '%s' "$doc" | json -l
echo "-- prune"
printf '%s' "$doc" | json -p
echo "-- brief"
printf '%s' "$doc" | json -b
echo "-- no head"
printf '%s' "$doc" | json -n
echo "-- solidus"
printf '%s' '{"url":"http:\/\/x\/y"}' | json -s -l
echo "-- layout"
printf '[\n  {"id": 1, "tags": ["a", "b"]},\n  {"id": 2,\t"tags": []}\n]\n' | json -l
printf '%s' '"é and \\ and \t"' | json
printf '%s' '0' | json
echo "-- errors"
printf '%s' '[1, 2' | json 2>/dev/null; echo "status $?"
printf '%s' '[1, 2' | json 2>&1 >/dev/null | grep -c 'EXPECTED , or \] GOT EOF'
printf '%s' '{"k" 1}' | json 2>/dev/null; echo "status $?"
printf '%s' '{1:2}' | json 2>/dev/null; echo "status $?"
printf '%s' '[1] 2' | json 2>/dev/null; echo "status $?"
printf '%s' '[1]' | json -x | head -2
