# source: https://www.pement.org/sed/sed1line.txt (SPECIAL APPLICATIONS)
# desc: remove nroff overstrikes (char, backspace) with \x08
# tags: gnu-ext (\xHH escape)
printf 'N\bNA\bAM\bME\bE plain\n' | sed 's/.\x08//g'
