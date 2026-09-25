# source: https://github.com/onetrueawk/awk/blob/master/testdir/p.42 and p.43
# desc: array accumulation; for (name in area) (output sorted since order is unspecified)
awk '/Asia/		{ pop["Asia"] += $3 }
/Africa/	{ pop["Africa"] += $3 }
END		{ print "Asian population in millions is", pop["Asia"]
		  print "African population in millions is", pop["Africa"] }' countries
awk 'BEGIN	{ FS = "\t" }
	{ area[$4] += $2 }
END	{ for (name in area)
		print name ":" area[name] }' countries | sort
