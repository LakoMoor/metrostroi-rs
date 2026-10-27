@echo off
rem Builds the release exe (icon embedded) and puts it next to the game folder as Metrostroi-rs.exe
cd /d "%~dp0..\sim"
cargo build --release -p viewer || exit /b 1
copy /y target\release\metrostroi-rs.exe "%~dp0..\Metrostroi-rs.exe" >nul
echo Built %~dp0..\Metrostroi-rs.exe
