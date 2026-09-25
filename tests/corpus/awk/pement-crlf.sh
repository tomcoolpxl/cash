# source: https://www.pement.org/awk/awk1line.txt (TEXT CONVERSION)
# desc: convert CRLF to LF and LF to CRLF
printf 'one\r\ntwo\r\n' | awk '{sub(/\r$/,"")};1' | tr '\r' R
printf 'one\ntwo\n' | awk '{sub(/$/,"\r")};1' | tr '\r' R
