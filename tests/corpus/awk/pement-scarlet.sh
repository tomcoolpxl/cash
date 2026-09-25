# source: https://www.pement.org/awk/awk1line.txt (TEXT CONVERSION)
# desc: change scarlet or ruby or puce to red
printf 'scarlet and ruby and puce and pink\n' | awk '{gsub(/scarlet|ruby|puce/, "red")}; 1'
