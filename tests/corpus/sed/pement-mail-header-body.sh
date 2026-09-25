# source: https://www.pement.org/sed/sed1line.txt (SPECIAL APPLICATIONS)
# desc: get message header, body, Subject, return address, parsed address
printf 'From: Jo <jo@x.org>\nSubject: Hello there\nReply-To: Rep (Mr R) <rep@y.org>\n\nbody line 1\nbody line 2\n' > msg
sed '/^$/q' msg; echo --
sed '1,/^$/d' msg; echo --
sed '/^Subject: */!d; s///;q' msg; echo --
sed '/^Reply-To:/q; /^From:/h; /./d;g;q' msg; echo --
sed '/^Reply-To:/q; /^From:/h; /./d;g;q' msg | sed 's/ *(.*)//; s/>.*//; s/.*[:<] *//'
