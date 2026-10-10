# alt_streams.rar, nt_security_crafted.rar

For 7z on NTFS alternate streams and ACLs (`7z_rar.sh`).

`alt_streams.rar` was made by WinRAR 7.23's `rar a -os` (NTFS streams kept) of a folder
`s` holding `a.txt` ("main data a") with a stream `note` ("stream text") and `b.txt`
("main data b"), both written by PowerShell's `Set-Content`.

`nt_security_crafted.rar` was written byte by byte by a script, not by WinRAR: 48 files
`fNN.txt` of one byte each, every one followed by a stored `ACL` service header holding
a self-relative security descriptor made up for the case. 7-Zip shows a descriptor as
its owner and group, by name where it has one, the entries of its SACL and DACL, and
its size. The cases:

- owners S-1-5-0 to S-1-5-23, the group a user's S-1-5-21-111-222-333-1001;
- owners S-1-5-32-544, 545, 553, 562, 563, 569, 573, 574 and 575 (the builtin groups
  and some 7-Zip has no name for);
- S-1-5-32 and S-1-5-32-544-1, S-1-5-80-1-2-3-4-5, S-1-5-21-1-2-3-500, S-1-1-0,
  S-1-16-12288, an authority of six bytes (S-1-0x010203040506-7-8), a SID of revision 2,
  and S-1-5 with no subauthority;
- a SACL and a DACL, an ACL of revision 4, a DACL flagged but absent, a DACL or owner
  offset past the end, a descriptor of revision 2, a group at offset 0, and 18 bytes
  that are no descriptor.

Every ACL entry grants SYSTEM all access; no SID in it is a real account's.
