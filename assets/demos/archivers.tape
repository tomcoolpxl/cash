# The archivers: tar, zip, 7z and rar make an archive of the same directory, built in.
Output archivers.gif
Require cash
Source "_settings.tape"

Type "cd .."
Enter
Sleep 300ms
Type "tar czf project.tar.gz project"
Enter
Sleep 900ms
Type "zip -qr project.zip project"
Enter
Sleep 900ms
Type "7z a project.7z project > /dev/null"
Enter
Sleep 900ms
Type "rar a project.rar project > /dev/null"
Enter
Sleep 900ms
Type "ls -l project.*"
Enter
Sleep 3000ms
Type "clear"
Enter
Type "unrar l project.rar"
Enter
Sleep 2800ms
Type "type tar zip 7z rar"
Enter
Sleep 2500ms
