# source: https://mywiki.wooledge.org/BashFAQ/073 (Examples of Filename Manipulation)
# desc: POSIX filename decomposition with printf "%0${#x}d"
FullPath=/path/to/name4afile-009.ext     # result:   #   /path/to/name4afile-009.ext
Filename=${FullPath##*/}                             #   name4afile-009.ext
PathPref=${FullPath%"$Filename"}                     #   /path/to/
FileStub=${Filename%.*}                              #   name4afile-009
FileExt=${Filename#"$FileStub"}                      #   .ext
FnumPossLeading0s=${FileStub##*[![:digit:]]}         #   009
FnumOnlyLeading0s=${FnumPossLeading0s%%[!0]*}        #   00
FileNumber=${FnumPossLeading0s#"$FnumOnlyLeading0s"} #   9
NextNumber=$(( FileNumber + 1 ))                     #   10
NextNumberWithLeading0s=$(printf "%0${#FnumPossLeading0s}d" "$NextNumber")
                                                     #   010
FileStubNoNum=${FileStub%"$FnumPossLeading0s"}       #   name4afile-
NewFullPath=${PathPref}New_${FileStubNoNum}${NextNumberWithLeading0s}${FileExt}
printf '%s\n' "$Filename" "$PathPref" "$FileStub" "$FileExt" "$FnumPossLeading0s" "$FnumOnlyLeading0s" "$FileNumber" "$NextNumberWithLeading0s" "$NewFullPath"
