# A regex RS, with a two-byte character across the 8 KiB point where the reader
# first looks for it.
BEGIN { RS = "--" }
{ print NR, length($0), substr($0, length($0) - 2) }
