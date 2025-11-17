import { fetch } from "@tauri-apps/plugin-http";

export const validateM3u8Url = async (url, options = {}) => {
    const resultTemplate = {
        valid: false,
        message: "",
        contentValid: false,
    };

    try {
        // 基础格式校验
        if (!/^https?:\/\//i.test(url)) {
            throw new Error("URL必须以http或https开头");
        }

        // 发起请求
        const response = await fetch(url, {
            timeout: options.timeout,
            headers: options.headers,
        });

        // HTTP 状态码验证
        if (!response.ok) {
            // 如果状态码不是 2xx，立即抛出错误
            throw new Error(`服务器返回错误状态码：${response.status}`);
        }

        // Content-Type 验证
        const contentType = response.headers.get("Content-Type");
        if (contentType && !/mpegurl|m3u8|plain|json/i.test(contentType)) {
            throw new Error(`Content-Type不匹配M3U8文件: ${contentType}`);
        }

        // 内容验证
        const text = await response.text();
        resultTemplate.contentValid = text.startsWith("#EXTM3U");
        if (!resultTemplate.contentValid) {
            throw new Error("文件内容缺少M3U8标识");
        }

        return {
            ...resultTemplate,
            valid: true,
            message: "有效的M3U8地址",
        };
    } catch (error) {
        console.error(error);
        return {
            ...resultTemplate,
            message: error.message || "验证过程中发生未知错误，可添加请求头后重试",
        };
    }
};
