# Who holds this port, who holds this file: ss, lsof, kill $(lsof -t ...), fuser -k.
Output ports.gif
Require cash
Require python
Source "_settings.tape"

Type "python -m http.server 8080 > /dev/null 2>&1 &"
Enter
Sleep 2000ms
Type "ss -ltn | grep -E 'Local|:8080'"
Enter
Sleep 1600ms
Type "lsof -i :8080"
Enter
Sleep 1600ms
Type "kill $(lsof -t -i :8080)"
Enter
Sleep 2500ms
Type "jobs"
Enter
Sleep 1500ms
Type "clear"
Enter
Type "cat scripts/hold.ps1"
Enter
Sleep 1500ms
Type "powershell -File scripts/hold.ps1 &"
Enter
Sleep 5000ms
Type "rm logs/app.log"
Enter
Sleep 1800ms
Type "fuser -v logs/app.log"
Enter
Sleep 1800ms
Type "fuser -k logs/app.log"
Enter
Sleep 1200ms
Type "rm logs/app.log && echo gone"
Enter
Sleep 2500ms
