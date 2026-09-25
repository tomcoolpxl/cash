# source: https://www.pement.org/sed/sed1line.txt (TEXT CONVERSION)
# desc: change scarlet or ruby or puce to red (most seds)
printf 'scarlet ruby puce pink\n' | sed 's/scarlet/red/g;s/ruby/red/g;s/puce/red/g'
