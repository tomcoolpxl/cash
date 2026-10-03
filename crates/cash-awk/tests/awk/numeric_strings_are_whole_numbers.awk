# Whether a field is a numeric string, which compares as a number, or a string, which
# compares as text: "9abc" < 10 is true only as text.
{ printf "%d %s\n", NR, ($0 < 10) }
