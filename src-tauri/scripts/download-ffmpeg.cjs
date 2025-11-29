const fs = require("fs");
const path = require("path");
const https = require("https");

function getFFmpegDownloadInfo() {
    const platform = process.platform;
    const arch = process.arch;

    // ffmpeg-static 提供的版本都是单个可执行文件，非常方便
    const baseUrl =
        "https://github.com/eugeneware/ffmpeg-static/releases/latest/download/";

    if (platform === "win32") {
        // Windows 64位
        return {
            url: baseUrl + "ffmpeg-win32-x64",
            fileName: "ffmpeg.exe",
        };
    } else if (platform === "darwin") {
        // macOS 根据架构选择
        if (arch === "arm64") {
            return {
                url: baseUrl + "ffmpeg-darwin-arm64",
                fileName: "ffmpeg",
            };
        } else {
            // Intel Mac
            return {
                url: baseUrl + "ffmpeg-darwin-x64",
                fileName: "ffmpeg",
            };
        }
    } else if (platform === "linux") {
        // Linux 64位
        return {
            url: baseUrl + "ffmpeg-linux-x64",
            fileName: "ffmpeg",
        };
    }

    throw new Error(`Unsupported platform: ${platform} ${arch}`);
}

function downloadFile(url, dest) {
    return new Promise((resolve, reject) => {
        const file = fs.createWriteStream(dest);

        https
            .get(url, (response) => {
                if (
                    response.statusCode === 302 ||
                    response.statusCode === 301
                ) {
                    // 处理重定向
                    downloadFile(response.headers.location, dest)
                        .then(resolve)
                        .catch(reject);
                    return;
                }

                if (response.statusCode !== 200) {
                    reject(
                        new Error(
                            `HTTP ${response.statusCode}: ${response.statusMessage}`,
                        ),
                    );
                    return;
                }

                const totalSize = parseInt(
                    response.headers["content-length"],
                    10,
                );
                let downloaded = 0;
                let lastPercent = -1;

                response.on("data", (chunk) => {
                    downloaded += chunk.length;

                    if (totalSize) {
                        const percent = Math.floor(
                            (downloaded / totalSize) * 100,
                        );

                        if (percent !== lastPercent) {
                            lastPercent = percent;

                            const barLength = 20; // 进度条长度
                            const filledLength = Math.floor(
                                (percent / 100) * barLength,
                            );
                            const bar =
                                "█".repeat(filledLength) +
                                "-".repeat(barLength - filledLength);

                            const downloadedMB = (
                                downloaded /
                                1024 /
                                1024
                            ).toFixed(2);
                            const totalMB = (totalSize / 1024 / 1024).toFixed(
                                2,
                            );

                            process.stdout.write(
                                `\r下载中: [${bar}] ${percent}% (${downloadedMB}MB / ${totalMB}MB)`,
                            );
                        }
                    }
                });

                response.pipe(file);

                file.on("finish", () => {
                    file.close(() => {
                        console.log("\n下载完成！");
                        resolve(dest);
                    });
                });
            })
            .on("error", (err) => {
                fs.unlink(dest, () => {});
                reject(err);
            });
    });
}

async function main() {
    const ffmpegDir = path.join(__dirname, "..", "bin");

    if (!fs.existsSync(ffmpegDir)) {
        fs.mkdirSync(ffmpegDir, { recursive: true });
    }

    const { url, fileName } = getFFmpegDownloadInfo();
    console.log(`Downloading ffmpeg from: ${url}`);

    const finalPath = path.join(ffmpegDir, fileName);

    // 检查是否已经存在
    if (fs.existsSync(finalPath)) {
        console.log("FFmpeg already exists, skipping download");
        return;
    }

    try {
        await downloadFile(url, finalPath);
        console.log("FFmpeg download completed successfully");
    } catch (error) {
        console.error("Failed to download FFmpeg:", error.message);
        process.exit(1);
    }
}

main().catch(console.error);
