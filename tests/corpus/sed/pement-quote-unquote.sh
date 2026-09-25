# source: https://www.pement.org/sed/sed1line.txt (SPECIAL APPLICATIONS)
# desc: add / remove a leading angle bracket and space (quote a message)
printf 'hi\nthere\n' | sed 's/^/> /' | tee q.txt
sed 's/^> //' q.txt
