---
see: test ls stat differences
---
## Description

`chmod MODE FILE...` changes what a file permits, as far as Windows can store it. A
Windows file has no Unix mode: it has a read-only attribute and an access list. So
`chmod` sets one thing, the read-only attribute, from the owner's write bit: `chmod u-w
f`, `chmod a-w f` or `chmod 444 f` make a file read-only, `chmod u+w f` or `chmod 644 f`
make it writable again.

## What Windows cannot store

- `chmod +x` and the execute bits of a number change nothing and return 0, silently, so
  `chmod +x deploy.sh && ./deploy.sh` works: whether a file can run is decided by its
  access list and its kind (an extension in `PATHEXT`, or a `#!` line), as `test -x`
  decides it.
- `chmod -x`, `-r`, setuid, setgid and the sticky bit warn and return 0: revoking them
  would take deny entries that can lock you out of your own file. `-f` keeps quiet.
- Group and other bits (`chmod go-w ~/.ssh`) change nothing and say nothing: Windows has
  no such bits per file.

## Examples

```
chmod +x build.sh
chmod a-w release/manifest.json     # read-only
chmod -R u+w vendor
```
