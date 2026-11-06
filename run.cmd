@echo off
setlocal
cd /d "%~dp0"
title Physics Lab - GPUI

if not defined GPUI_FXC_PATH (
  for /f "usebackq delims=" %%F in (`powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\find-fxc.ps1"`) do set "GPUI_FXC_PATH=%%F"
)
if not defined GPUI_FXC_PATH (
  echo fxc.exe not found. Install the "Windows 11 SDK" component from the Visual Studio Installer.
  echo fxc.exe not found > build.log
  pause
  exit /b 1
)
echo Using fxc: %GPUI_FXC_PATH%
echo Building release... output is in build.log (first build takes a few minutes)
echo Using fxc: %GPUI_FXC_PATH% > build.log
cargo build --release >> build.log 2>&1
if errorlevel 1 (
  echo BUILD FAILED >> build.log
  echo Build failed - see build.log
  pause
  exit /b 1
)
echo BUILD OK >> build.log
start "" "target\release\gpui-demo.exe"
