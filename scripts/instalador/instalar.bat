@echo off
rem Instala presupuestos-bc3 para este usuario (no necesita administrador).
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0instalar.ps1"
echo.
pause
