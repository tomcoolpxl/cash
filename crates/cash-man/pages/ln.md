---
name: ln
summary: Make links between files: symbolic links, junctions or hard links.
kind: Bundled coreutils, from uutils
see: test link paths
draft: yes
---

## How it works

`ln TARGET LINK` makes a hard link; `ln -s TARGET LINK` a symbolic link.

## Different in cash

Windows lets an account make symbolic links only with Developer Mode on, or elevated.
So `ln -s`:

- makes a real symbolic link when Windows allows it;
- for a directory, makes a junction otherwise, which needs no privilege and which `test -L`
  reports as a link;
- for a file, fails otherwise, and says that Developer Mode is the fix. It never falls
  back to a hard link, which would answer `test -L` wrongly.

cash follows symbolic links and junctions as Windows does. `rm -r` on a directory that
holds one takes the link away, never what it points to, whether that is still there or
not.
