@echo off
setlocal
set "APP=%~dp0"
if "%APP:~-1%"=="\" set "APP=%APP:~0,-1%"
set "DLL64=qingjian_tsf-__VERSION__.dll"
set "DLL32=qingjian_tsf-__VERSION__-x86.dll"

reg query "HKU\S-1-5-19" >nul 2>&1
if not errorlevel 1 goto :elevated
echo   Requesting administrator privileges...
powershell -NoProfile -Command "Start-Process -FilePath '%~f0' -Verb RunAs -Wait"
exit /b

:elevated
echo.
echo   Qingjian IME - uninstall
echo.
taskkill /IM qingjian-server.exe /F >nul 2>&1
taskkill /IM qingjian-settings.exe /F >nul 2>&1
timeout /t 1 /nobreak >nul

regsvr32 /u /s "%APP%\%DLL64%" >nul 2>&1
"%SystemRoot%\SysWOW64\regsvr32.exe" /u /s "%APP%\%DLL32%" >nul 2>&1
rem Older versions left their DLLs behind; unregister those too.
for %%F in ("%APP%\qingjian_tsf*.dll") do regsvr32 /u /s "%%F" >nul 2>&1
powershell -NoProfile -EncodedCommand __REMOVE_B64__ >nul 2>&1

echo Removed the input method registration and the shortcuts.
echo.
set "DELUSER=N"
set /p "DELUSER=Also delete your personal data in %APPDATA%\Qingjian (learned words, terms, settings)? [y/N] "
if /i not "%DELUSER%"=="y" (
  echo   Kept %APPDATA%\Qingjian
) else (
  rmdir /s /q "%APPDATA%\Qingjian" >nul 2>&1
  echo   Deleted %APPDATA%\Qingjian
)

echo.
echo Removing files from %APP% ...
start "" /min cmd /c "timeout /t 2 >nul & rmdir /s /q ""%APP%"""
echo Done. If some DLLs were still loaded, the folder is removed after you sign out or reboot.
echo.
pause
exit /b 0
