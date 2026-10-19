# source: https://github.com/dominictarr/JSON.sh/tree/0d5e5c7 (the script `JSON.sh` as the fixture, unchanged)
# desc: JSON.sh run as a program through sh, where json-sh.sh sources it: the text cut into tokens with grep -o, a recursive descent of functions reading them one at a time, every path and value printed, -l leaves only, -b brief, -p pruned, -n without the head, -s solidus, and malformed input refused with its status
printf '%s\n' '{"name": "cash", "version": [1, 34, 0], "ok": true,' \
  '"tags": {"shell": "bash", "os": "windows"}, "empty": [], "none": {},' \
  '"nested": {"a": {"b": null}}, "text": "line\nbreak \"quoted\" a\/b",' \
  '"num": -12.5e3}' > in.json
sh ./JSON.sh < in.json; echo "status: $?"
sh ./JSON.sh -l < in.json
sh ./JSON.sh -b < in.json
sh ./JSON.sh -p < in.json
sh ./JSON.sh -n -l < in.json
sh ./JSON.sh -s -l < in.json
echo '[1, 2' | sh ./JSON.sh; echo "status: $?"
echo '{"a": tru}' | sh ./JSON.sh; echo "status: $?"
echo '{"a" 1}' | sh ./JSON.sh; echo "status: $?"
printf '[\n  "x",\n  {"y": [true, false]}\n]\n' | sh ./JSON.sh -l; echo "status: $?"
