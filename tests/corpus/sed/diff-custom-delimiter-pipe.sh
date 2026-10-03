# source: tests/sed-differential.sh, case custom-delimiter-pipe, frozen into the corpus
# desc: custom-delimiter-pipe
printf '%b' '/usr/local/bin:/usr/bin\n' | sed 's|/usr/local/bin|/opt/bin|g'
