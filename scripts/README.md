# 版本管理脚本使用说明

本目录包含用于统一管理项目版本号的脚本。

## 📁 文件说明

- `update-version.js` - Node.js 版本（跨平台）
- `update-version.ps1` - PowerShell 版本（Windows 推荐）
- `README.md` - 本说明文档

## 🎯 功能

一键更新所有配置文件中的版本号：

- ✅ `package.json`
- ✅ `src-tauri/Cargo.toml`
- ✅ `src-tauri/tauri.conf.json`
- ✅ `.env` (VITE_APP_VERSION)

## 🚀 使用方法

### 方法 1：使用 npm script（推荐）

```bash
# 更新版本号到 1.2.0
npm run version 1.2.0
```

### 方法 2：直接运行 Node.js 脚本

```bash
node scripts/update-version.js 1.2.0
```

### 方法 3：使用 PowerShell 脚本（Windows）

```powershell
# PowerShell
.\scripts\update-version.ps1 1.2.0

# 或使用 pwsh（跨平台 PowerShell）
pwsh scripts/update-version.ps1 1.2.0
```

## 📋 完整发版流程

### 1. 更新版本号

```bash
npm run version 1.2.0
```

### 2. 检查更改

```bash
git diff
```

确认以下文件都已更新：
- ✅ package.json
- ✅ Cargo.toml
- ✅ tauri.conf.json
- ✅ .env

### 3. 提交更改

```bash
git add .
git commit -m "chore: bump version to 1.2.0"
```

### 4. 创建 Git 标签

```bash
git tag v1.2.0
```

### 5. 推送到远程仓库

```bash
# 推送代码
git push

# 推送标签
git push --tags
```

### 6. 构建发布包

```bash
# 构建 Tauri 应用
npm run tauri build
```

## 📝 版本号格式

遵循 [语义化版本](https://semver.org/lang/zh-CN/) 规范：

### 标准版本

```
major.minor.patch
```

**示例：**
- `1.0.0` - 主版本
- `1.1.0` - 次版本（新功能）
- `1.1.1` - 修订版本（Bug 修复）

### 预发布版本

```
major.minor.patch-prerelease
```

**示例：**
- `2.0.0-alpha.1` - Alpha 版本
- `2.0.0-beta.1` - Beta 版本
- `2.0.0-rc.1` - Release Candidate


## ⚠️ 注意事项

### 1. 版本号格式验证

脚本会自动验证版本号格式，不符合规范的版本号会被拒绝：

```bash
# ❌ 错误示例
npm run version 1.2        # 缺少 patch 版本
npm run version v1.2.0     # 不应包含 'v' 前缀
npm run version 1.2.0.0    # 版本号过长

# ✅ 正确示例
npm run version 1.2.0
```