' Double-click this while the quest window is open on the gem.
' No window appears. A zip is saved on the Desktop.
Set fso = CreateObject("Scripting.FileSystemObject")
folder = fso.GetParentFolderName(WScript.ScriptFullName)
ps1 = folder & "\gem-highlight-dump.ps1"
Set sh = CreateObject("WScript.Shell")
code = sh.Run("powershell.exe -NoProfile -ExecutionPolicy Bypass -File """ & ps1 & """", 0, True)
WScript.Quit code
