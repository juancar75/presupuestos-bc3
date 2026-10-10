@echo off
rem Compila (la primera vez tarda unos minutos) y abre la interfaz de escritorio.
cd /d "%~dp0\..\.."
cargo build --release -p ppto-gui
if errorlevel 1 (
  echo.
  echo *** ERROR al compilar: revisa el mensaje anterior ***
  pause
  exit /b 1
)
start "" "target\release\ppto-gui.exe" %*
