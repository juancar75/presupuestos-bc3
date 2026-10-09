@echo off
chcp 65001 >nul
cd /d "%~dp0\..\.."
echo === 1/2 Pruebas automaticas ===
cargo test --workspace
if errorlevel 1 goto error
echo.
echo === 2/2 Demo: caso suelo radiante ===
cargo run -q -p ppto-cli -- demo
if errorlevel 1 goto error
goto fin
:error
echo *** ERROR: revisa el mensaje anterior ***
:fin
pause
