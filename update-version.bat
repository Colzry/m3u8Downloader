@echo off
REM 版本号更新脚本 (Windows 批处理版本)
REM 用法: update-version.bat <version>
REM 例如: update-version.bat 1.2.0

setlocal enabledelayedexpansion

if "%~1"=="" (
    echo.
    echo ❌ 错误：请提供版本号！
    echo.
    echo 📖 使用方法：update-version.bat ^<version^>
    echo 📖 示例：update-version.bat 1.2.0
    echo.
    pause
    exit /b 1
)

set VERSION=%~1

echo.
echo 🚀 开始更新版本号到 %VERSION%...
echo.

REM 检查 Node.js 是否安装
where node >nul 2>nul
if %ERRORLEVEL% neq 0 (
    echo ❌ 错误：未找到 Node.js！
    echo 📖 请先安装 Node.js: https://nodejs.org/
    echo.
    pause
    exit /b 1
)

REM 运行 Node.js 脚本
node scripts\update-version.cjs %VERSION%

if %ERRORLEVEL% equ 0 (
    echo.
    echo ✅ 版本号更新成功！
    echo.
) else (
    echo.
    echo ❌ 版本号更新失败！
    echo.
)

pause
exit /b %ERRORLEVEL%