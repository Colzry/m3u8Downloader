#!/usr/bin/env pwsh

<#
.SYNOPSIS
    版本号统一更新脚本 (PowerShell 版本)

.DESCRIPTION
    一键更新所有配置文件中的版本号，包括：
    - package.json
    - Cargo.toml
    - tauri.conf.json
    - .env

.PARAMETER Version
    新的版本号，格式：major.minor.patch (例如：1.2.0)

.EXAMPLE
    .\scripts\update-version.ps1 1.2.0
    更新所有配置文件的版本号到 1.2.0

.EXAMPLE
    .\scripts\update-version.ps1 2.0.0-beta.1
    更新版本号到 2.0.0-beta.1 (预发布版本)
#>

param(
    [Parameter(Mandatory=$true, Position=0)]
    [string]$Version
)

# 设置错误时停止
$ErrorActionPreference = "Stop"

# 验证版本号格式 (semver)
if ($Version -notmatch '^\d+\.\d+\.\d+(-[a-zA-Z0-9.-]+)?(\+[a-zA-Z0-9.-]+)?$') {
    Write-Host "❌ 错误：版本号格式不正确！" -ForegroundColor Red
    Write-Host "📖 版本号格式应为：major.minor.patch" -ForegroundColor Yellow
    Write-Host "📖 例如：1.2.0 或 1.2.0-beta.1" -ForegroundColor Yellow
    exit 1
}

# 获取项目根目录
$RootDir = Split-Path -Parent $PSScriptRoot

Write-Host "`n🚀 开始更新版本号到 $Version...`n" -ForegroundColor Cyan

$SuccessCount = 0
$FailCount = 0

# =====================================================
# 1. 更新 package.json
# =====================================================
try {
    $PackageJsonPath = Join-Path $RootDir "package.json"
    if (Test-Path $PackageJsonPath) {
        $PackageJson = Get-Content $PackageJsonPath -Raw | ConvertFrom-Json
        $OldVersion = $PackageJson.version
        $PackageJson.version = $Version
        $PackageJson | ConvertTo-Json -Depth 100 | Set-Content $PackageJsonPath -Encoding UTF8
        Write-Host "  📦 package.json: $OldVersion → $Version" -ForegroundColor Green
        $SuccessCount++
    } else {
        Write-Host "  ⚠️  package.json 不存在，跳过" -ForegroundColor Yellow
    }
} catch {
    Write-Host "  ❌ 更新 package.json 失败: $_" -ForegroundColor Red
    $FailCount++
}

# =====================================================
# 2. 更新 Cargo.toml
# =====================================================
try {
    $CargoTomlPath = Join-Path $RootDir "src-tauri\Cargo.toml"
    if (Test-Path $CargoTomlPath) {
        $CargoToml = Get-Content $CargoTomlPath -Raw
        if ($CargoToml -match 'version\s*=\s*"([\d.+-]+)"') {
            $OldVersion = $Matches[1]
            $CargoToml = $CargoToml -replace 'version\s*=\s*"[\d.+-]+"', "version = `"$Version`""
            $CargoToml | Set-Content $CargoTomlPath -Encoding UTF8 -NoNewline
            Write-Host "  📦 Cargo.toml: $OldVersion → $Version" -ForegroundColor Green
            $SuccessCount++
        } else {
            throw "在 Cargo.toml 中找不到 version 字段"
        }
    } else {
        Write-Host "  ⚠️  Cargo.toml 不存在，跳过" -ForegroundColor Yellow
    }
} catch {
    Write-Host "  ❌ 更新 Cargo.toml 失败: $_" -ForegroundColor Red
    $FailCount++
}

# =====================================================
# 3. 更新 tauri.conf.json
# =====================================================
try {
    $TauriConfPath = Join-Path $RootDir "src-tauri\tauri.conf.json"
    if (Test-Path $TauriConfPath) {
        $TauriConf = Get-Content $TauriConfPath -Raw | ConvertFrom-Json
        $OldVersion = $TauriConf.version
        $TauriConf.version = $Version
        $TauriConf | ConvertTo-Json -Depth 100 | Set-Content $TauriConfPath -Encoding UTF8
        Write-Host "  📦 tauri.conf.json: $OldVersion → $Version" -ForegroundColor Green
        $SuccessCount++
    } else {
        Write-Host "  ⚠️  tauri.conf.json 不存在，跳过" -ForegroundColor Yellow
    }
} catch {
    Write-Host "  ❌ 更新 tauri.conf.json 失败: $_" -ForegroundColor Red
    $FailCount++
}

# =====================================================
# 4. 更新 .env
# =====================================================
try {
    $EnvPath = Join-Path $RootDir ".env"
    if (Test-Path $EnvPath) {
        $EnvContent = Get-Content $EnvPath -Raw
        if ($EnvContent -match 'VITE_APP_VERSION=(.+)') {
            $OldVersion = $Matches[1].Trim()
            $EnvContent = $EnvContent -replace 'VITE_APP_VERSION=.+', "VITE_APP_VERSION=$Version"
            Write-Host "  📦 .env: $OldVersion → $Version" -ForegroundColor Green
        } else {
            # 如果不存在，则添加
            $EnvContent = $EnvContent.TrimEnd() + "`nVITE_APP_VERSION=$Version`n"
            Write-Host "  📦 .env: (新增) → $Version" -ForegroundColor Green
        }
        $EnvContent | Set-Content $EnvPath -Encoding UTF8 -NoNewline
        $SuccessCount++
    } else {
        Write-Host "  ⚠️  .env 不存在，跳过" -ForegroundColor Yellow
    }
} catch {
    Write-Host "  ❌ 更新 .env 失败: $_" -ForegroundColor Red
    $FailCount++
}

# =====================================================
# 显示结果
# =====================================================
Write-Host "`n$('=' * 50)" -ForegroundColor Cyan
Write-Host "✅ 成功更新 $SuccessCount 个文件" -ForegroundColor Green
if ($FailCount -gt 0) {
    Write-Host "❌ 失败 $FailCount 个文件" -ForegroundColor Red
}
Write-Host "$('=' * 50)`n" -ForegroundColor Cyan

if ($FailCount -eq 0) {
    Write-Host "🎉 版本号更新完成！" -ForegroundColor Green
    Write-Host "`n💡 下一步：" -ForegroundColor Yellow
    Write-Host "   1. 检查更改：git diff" -ForegroundColor White
    Write-Host "   2. 提交更改：git add . && git commit -m `"chore: bump version to $Version`"" -ForegroundColor White
    Write-Host "   3. 创建标签：git tag v$Version" -ForegroundColor White
    Write-Host "   4. 推送代码：git push && git push --tags`n" -ForegroundColor White
    exit 0
} else {
    Write-Host "⚠️  部分文件更新失败，请检查错误信息" -ForegroundColor Yellow
    exit 1
}