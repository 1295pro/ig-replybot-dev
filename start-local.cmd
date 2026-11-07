@echo off
setlocal
cd /d "%~dp0"
if exist "%~dp0ig-reply-bot.exe" (
  "%~dp0ig-reply-bot.exe" --local
) else (
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\start-local.ps1"
)
if errorlevel 1 (
  echo.
  echo Local bot did not start. Read the error above.
  pause
)
