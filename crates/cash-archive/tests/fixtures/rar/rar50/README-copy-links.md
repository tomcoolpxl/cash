# copy_links_*.rar

Made by WinRAR 7.23's `rar` with `-oi:1` (identical files saved as references, from
one byte up), for 7z's copy links (`7z_rar.sh`). The members are printable random
bytes made by awk's `srand(n)`/`rand()`, all dated 2024-01-01 00:00:00 UTC:

- `s/a.bin` 3000 bytes, `s/b.bin` 4000, `s/c.bin` 2000, `s/d.bin` a copy of `s/b.bin`,
  `s/e.bin` 1000, and the folder `s`. `s/d.bin` is a file reference to `s/b.bin`.
- `t/a.bin` 3000 bytes, `t/b.bin` 2000, `t/c.bin` 2000, `t/d.bin` 3000, `t/e.bin` a
  copy of `t/a.bin`, `t/f.bin` `t/d.bin` and 100 bytes more, and the folder `t`.
  `t/e.bin` is a file reference to `t/a.bin`.

The commands, run in the folder holding `s` and `t` (`-ds` keeps the names' order):

    rar a -oi:1 -ds copy_links_plain.rar s
    rar a -s -oi:1 -ds copy_links_solid.rar s
    rar a -s=2f -oi:1 -ds copy_links_groups.rar s
    rar a -s=3f -oi:1 -ds copy_links_mid_stream.rar t
    rar a -s -oi:1 -ds -ppassword copy_links_password.rar s

In `copy_links_groups.rar`, `s/c.bin` starts a new solid group, so no stream goes on
from `s/b.bin`. In `copy_links_mid_stream.rar`, `t/d.bin` starts the second group, `t/e.bin`
points back at `t/a.bin` in the first, and `t/f.bin`'s matches reach back into `t/d.bin`.
