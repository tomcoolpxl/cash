# source: https://www.pement.org/sed/sed1line.txt (FILE SPACING)
# desc: undo double-spacing (assumes even-numbered lines are always blank)
printf 'a\n\nb\n\nc\n\n' | sed 'n;d'
