# solid_text.rar

Made by WinRAR 7.23's `rar` as a solid archive of three text files whose matches reach
back into the files before them, for 7z on damaged solid data (`7z_rar.sh`). Each file
is 600 lines of `word number word`, the words from a dozen Greek letter names, made by
awk's `srand(n)`/`rand()` with n = 1, 2, 3; all dated 2024-01-01 00:00:00 UTC:

- `s/t1.txt` 9034 bytes, `s/t2.txt` 9019, `s/t3.txt` 8993, and the folder `s`.

The command, run in the folder holding `s` (`-ds` keeps the names' order):

    rar a -s -ds solid_text.rar s
