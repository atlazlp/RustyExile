@echo off
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0gem-highlight-dump.ps1"
if errorlevel 1 (
  echo.
  echo The dump failed. Screenshot this window and send it with the zip.
  pause
)
