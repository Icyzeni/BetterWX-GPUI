@echo off
chcp 65001 >nul
cd /d "%~dp0"

echo BetterWX-GPUI 一键构建（下载原版核心 + 编译 + 打包 zip）
echo.

powershell -NoProfile -ExecutionPolicy Bypass -File "..\\scripts\\build.ps1"
if errorlevel 1 (
  echo.
  echo [X] 构建失败。
  pause
  exit /b 1
)

echo.
pause
