---
see: test link paths
---
## Description

`ln TARGET LINK` makes a hard link; `ln -s TARGET LINK` a symbolic link.

## Windows notes

Windows lets an account make symbolic links only with Developer Mode on, or elevated.
So `ln -s`:

- makes a real symbolic link when Windows allows it;
- for a folder, makes a junction otherwise, which needs no privilege and which `test -L`
  reports as a link;
- for a file, fails otherwise, and says that Developer Mode is the fix. It never falls
  back to a hard link, which would answer `test -L` wrongly.

cash follows symbolic links and junctions as Windows does.
