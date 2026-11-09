@echo off
setlocal
set "HERE=%~dp0"
set "APP=%ProgramFiles%\Qingjian"
set "DLL64=qingjian_tsf-__VERSION__.dll"
set "DLL32=qingjian_tsf-__VERSION__-x86.dll"

rem ------------------------------------------------------------------ admin
rem regsvr32 writes HKCR, so the whole install must run elevated.
rem The probe must not depend on a service (net session fails when the
rem Server service is off), so use the classic registry view check.
reg query "HKU\S-1-5-19" >nul 2>&1
if not errorlevel 1 goto :elevated
echo   Requesting administrator privileges...
powershell -NoProfile -Command "Start-Process -FilePath '%~f0' -Verb RunAs -Wait"
exit /b

:elevated
echo.
echo   Qingjian IME - Windows setup
echo   Target: %APP%
echo.

if not exist "%HERE%payload.zip" goto :nopayload
if not exist "%SystemRoot%\System32\tar.exe" goto :notar

rem ------------------------------------------------------- stop and free files
echo [1/6] Stopping running Qingjian processes...
taskkill /IM qingjian-settings.exe /F >nul 2>&1
taskkill /IM qingjian-server.exe /F >nul 2>&1
timeout /t 1 /nobreak >nul

rem The TSF DLL is loaded into every app process, so it cannot be overwritten.
rem Renaming works even while it is loaded and frees the old name for the new
rem file. Same trick as the Inno installer.
echo [2/6] Freeing locked files...
if not exist "%APP%" mkdir "%APP%"
if exist "%APP%\qingjian-server.exe" move /y "%APP%\qingjian-server.exe" "%APP%\qingjian-server.old-%RANDOM%.exe" >nul 2>&1
if exist "%APP%\%DLL64%" move /y "%APP%\%DLL64%" "%APP%\qingjian_tsf.old-%RANDOM%.dll" >nul 2>&1
if exist "%APP%\%DLL32%" move /y "%APP%\%DLL32%" "%APP%\qingjian_tsf.old-%RANDOM%.dll" >nul 2>&1

echo [3/6] Copying files...
"%SystemRoot%\System32\tar.exe" -xf "%HERE%payload.zip" -C "%APP%"
if errorlevel 1 goto :copyfailed

echo [4/6] Granting app-container access...
icacls "%APP%" /grant *S-1-15-2-1:(OI)(CI)RX /T /C /Q >nul 2>&1

echo [5/6] Registering the input method...
regsvr32 /s "%APP%\%DLL64%"
if errorlevel 1 goto :regfailed
"%SystemRoot%\SysWOW64\regsvr32.exe" /s "%APP%\%DLL32%"
if errorlevel 1 goto :regfailed

rem Older versions left their DLLs behind. The profile now points at the new
rem DLL, so the old ones are dead weight (still locked -> deleted on reboot).
for %%F in ("%APP%\qingjian_tsf*.dll") do (
  if /i not "%%~nxF"=="%DLL64%" if /i not "%%~nxF"=="%DLL32%" del /f /q "%%F" >nul 2>&1
)
for %%F in ("%APP%\qingjian-server.old-*.exe") do del /f /q "%%F" >nul 2>&1

echo [6/6] Creating shortcuts...
powershell -NoProfile -EncodedCommand __SHORTCUT_B64__ >nul 2>&1

rem Start through Explorer so the server runs at the signed-in user's
rem integrity level instead of inheriting this elevated window.
explorer.exe "%APP%\qingjian-server.exe"
timeout /t 2 /nobreak >nul

echo.
echo   Installed to %APP%
echo.
echo   Sign out and back in (or reboot) so every app picks up the input method.
echo   Then switch to Qingjian with Win+Space or the language bar.
echo.
echo   Read: %APP%\readme.txt
echo   Uninstall: Start menu - Qingjian - Uninstall, or %APP%\uninstall.cmd
echo.
pause
exit /b 0

:nopayload
echo payload.zip not found next to the installer - the package is incomplete.
pause
exit /b 1

:notar
echo tar.exe not found (needs Windows 10 1803 or newer).
pause
exit /b 1

:copyfailed
echo.
echo Copying files failed. Close every app that may be using Qingjian and run again.
pause
exit /b 1

:regfailed
echo.
echo Registering the input method failed.
pause
exit /b 1
