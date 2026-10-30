# Shared by every demo tape (`Source "_settings.tape"`, after its own `Set Width` and
# `Set Height`): the font, the colours of Windows Terminal's Campbell scheme, then cash
# started from cmd, hidden. VHS starts a shell from a fixed list, so the tape starts cmd
# and types `cash` into it.

Set Shell cmd
Set FontFamily "Cascadia Mono"
Set FontSize 20
Set Padding 20
Set Framerate 15
Set TypingSpeed 60ms
Set Theme { "name": "Campbell", "background": "#0c0c0c", "foreground": "#cccccc", "cursor": "#ffffff", "selection": "#3a3a3a", "black": "#0c0c0c", "red": "#c50f1f", "green": "#13a10e", "yellow": "#c19c00", "blue": "#0037da", "magenta": "#881798", "cyan": "#3a96dd", "white": "#cccccc", "brightBlack": "#767676", "brightRed": "#e74856", "brightGreen": "#16c60c", "brightYellow": "#f9f1a5", "brightBlue": "#3b78ff", "brightMagenta": "#b4009e", "brightCyan": "#61d6d6", "brightWhite": "#f2f2f2" }

Hide
Type "set LOCALAPPDATA=%PUBLIC%\cash-demo\.local"
Enter
Type "cash --rcfile demo.bashrc"
Enter
Sleep 1500ms
Type "clear"
Enter
Sleep 300ms
Show
