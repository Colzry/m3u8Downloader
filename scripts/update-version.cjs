#!/usr/bin/env node

/**
 * 版本号统一更新脚本
 * 一键更新所有配置文件中的版本号
 * 
 * 使用方法：
 *   node scripts/update-version.js <version>
 *   例如：node scripts/update-version.js 1.2.0
 * 
 * 或使用 npm script：
 *   npm run version 1.2.0
 */

const fs = require('fs');
const path = require('path');

// 获取命令行参数
const newVersion = process.argv[2];

if (!newVersion) {
  console.error('❌ 错误：请提供版本号！');
  console.log('📖 使用方法：node scripts/update-version.js <version>');
  console.log('📖 示例：node scripts/update-version.js 1.2.0');
  process.exit(1);
}

// 验证版本号格式 (semver)
const versionRegex = /^\d+\.\d+\.\d+(-[a-zA-Z0-9.-]+)?(\+[a-zA-Z0-9.-]+)?$/;
if (!versionRegex.test(newVersion)) {
  console.error('❌ 错误：版本号格式不正确！');
  console.log('📖 版本号格式应为：major.minor.patch');
  console.log('📖 例如：1.2.0 或 1.2.0-beta.1');
  process.exit(1);
}

// 项目根目录
const rootDir = path.join(__dirname, '..');

// 需要更新的文件配置
const files = [
  {
    name: 'package.json',
    path: path.join(rootDir, 'package.json'),
    update: (content, version) => {
      const pkg = JSON.parse(content);
      const oldVersion = pkg.version;
      pkg.version = version;
      console.log(`  📦 package.json: ${oldVersion} → ${version}`);
      return JSON.stringify(pkg, null, 2) + '\n';
    }
  },
  {
    name: 'Cargo.toml',
    path: path.join(rootDir, 'src-tauri', 'Cargo.toml'),
    update: (content, version) => {
      const versionRegex = /^version\s*=\s*"[\d.+-]+"/m;
      const match = content.match(versionRegex);
      if (match) {
        const oldVersion = match[0].match(/"(.+)"/)[1];
        console.log(`  📦 Cargo.toml: ${oldVersion} → ${version}`);
        return content.replace(versionRegex, `version = "${version}"`);
      }
      throw new Error('在 Cargo.toml 中找不到 version 字段');
    }
  },
  {
    name: 'tauri.conf.json',
    path: path.join(rootDir, 'src-tauri', 'tauri.conf.json'),
    update: (content, version) => {
      const config = JSON.parse(content);
      const oldVersion = config.version;
      config.version = version;
      console.log(`  📦 tauri.conf.json: ${oldVersion} → ${version}`);
      return JSON.stringify(config, null, 2) + '\n';
    }
  },
  {
    name: '.env',
    path: path.join(rootDir, '.env'),
    update: (content, version) => {
      const versionRegex = /^VITE_APP_VERSION=.+$/m;
      const match = content.match(versionRegex);
      if (match) {
        const oldVersion = match[0].split('=')[1];
        console.log(`  📦 .env: ${oldVersion} → ${version}`);
        return content.replace(versionRegex, `VITE_APP_VERSION=${version}`);
      }
      // 如果不存在，则添加
      console.log(`  📦 .env: (新增) → ${version}`);
      return content.trim() + `\nVITE_APP_VERSION=${version}\n`;
    }
  }
];

console.log(`\n🚀 开始更新版本号到 ${newVersion}...\n`);

let successCount = 0;
let failCount = 0;

// 更新所有文件
files.forEach(file => {
  try {
    // 检查文件是否存在
    if (!fs.existsSync(file.path)) {
      console.warn(`⚠️  ${file.name} 不存在，跳过`);
      return;
    }

    // 读取文件内容
    const content = fs.readFileSync(file.path, 'utf8');

    // 更新版本号
    const newContent = file.update(content, newVersion);

    // 写入文件
    fs.writeFileSync(file.path, newContent, 'utf8');

    successCount++;
  } catch (error) {
    console.error(`❌ 更新 ${file.name} 失败: ${error.message}`);
    failCount++;
  }
});

console.log('\n' + '='.repeat(50));
console.log(`✅ 成功更新 ${successCount} 个文件`);
if (failCount > 0) {
  console.log(`❌ 失败 ${failCount} 个文件`);
}
console.log('='.repeat(50) + '\n');

if (failCount === 0) {
  console.log('🎉 版本号更新完成！');
  console.log(`\n💡 下一步：`);
  console.log(`   1. 检查更改：git diff`);
  console.log(`   2. 提交更改：git add . && git commit -m "chore: bump version to ${newVersion}"`);
  console.log(`   3. 创建标签：git tag v${newVersion}`);
  console.log(`   4. 推送代码：git push && git push --tags\n`);
  process.exit(0);
} else {
  console.error('⚠️  部分文件更新失败，请检查错误信息');
  process.exit(1);
}