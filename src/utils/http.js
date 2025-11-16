import { fetch } from "@tauri-apps/plugin-http";
import { createDiscreteApi } from "naive-ui";

// 创建一个离散 API 实例
const { message } = createDiscreteApi(["message"]);

/**
 * 核心请求函数，用于处理所有 HTTP 请求
 * @param {string} url 完整的请求地址 (不再包含 baseURL)
 * @param {object} options fetch 的配置项
 * @returns {Promise<any>} 返回 JSON 解析后的数据
 */
const request = async (url, options = {}) => {
    // 设置默认超时时间
    const defaultOptions = {
        timeout: 6000,
        headers: {
            "Content-Type": "application/json",
            ...options.headers,
        },
        // 允许外部传入其他 fetch 选项，如 signal, body 等
        ...options,
    };

    try {
        const response = await fetch(url, defaultOptions);

        if (response.status >= 200 && response.status < 300) {
            // 尝试返回 JSON，如果响应体为空或不是 JSON，则返回响应对象
            try {
                return await response.json();
            } catch (e) {
                // 如果不是 JSON 响应，可能是一个 204 No Content 或其他非 JSON 成功响应
                return response; 
            }
        } else {
            // 处理非 2xx 状态码
            throw new Error(`请求失败，状态码: ${response.status} (${response.statusText})`);
        }
    } catch (error) {
        // 处理网络错误、超时、AbortController 错误等
        console.error("HTTP request failed:", error);
        
        // 弹窗提示
        let errMsg = "请求失败";
        if (error.message.includes("timeout")) {
            errMsg = "请求超时";
        } else if (error.message.includes("401") || error.message.includes("403")) {
            errMsg = "权限不足或未认证";
        }
        message.error(errMsg);
        
        throw error;
    }
};

/**
 * 封装 GET 请求
 * @param {string} url 完整的请求地址
 * @param {object} options fetch 的配置项
 */
export const get = async (url, options = {}) => {
    return request(url, {
        method: "GET",
        ...options,
    });
};

/**
 * 封装 POST 请求
 * @param {string} url 完整的请求地址
 * @param {any} body 请求体数据 (将自动 JSON.stringify)
 * @param {object} options fetch 的配置项
 */
export const post = async (url, body, options = {}) => {
    return request(url, {
        method: "POST",
        body: JSON.stringify(body),
        ...options,
    });
};

// 你可以根据需要添加更多 HTTP 方法的封装
// 下面为类似axios的封装，不能直接使用，因为返回值多包了一层Promise
export const http = (opts = {}) => {
    return new Promise((resolve, reject) => {
        const { url, method, query, data, headers, callback } = opts;
        fetch(baseURL + url, {
            method: method || "GET",
            headers: {
                "content-type": "application/json",
                ...headers,
            },
            timeout: 6000,
            query: query,
            body: data ? JSON.stringify(data) : undefined,
        })
            .then((res) => {
                callback && callback(res);
                resolve(res);
            })
            .catch((e) => {
                reject(e);
            });
    });
};
