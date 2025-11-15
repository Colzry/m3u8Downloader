import { fetch } from "@tauri-apps/plugin-http";
import { createDiscreteApi } from "naive-ui";

// 创建一个离散 API 实例
const { message } = createDiscreteApi(["message"]);
const baseURL = "http://localhost:8848";

// 封装 GET 请求
export const get = async (path, options = {}) => {
    const url = baseURL + path;
    try {
        const response = await fetch(url, {
            method: "GET",
            timeout: 6000, //请求不能超过6S
            ...options,
        });

        if (response.status >= 200 && response.status < 300) {
            return await response.json();
        } else {
            throw new Error(`Request failed with status: ${response.status}`);
        }
    } catch (error) {
        console.error("GET request failed:", error);
        message.error("数据请求失败");
        throw error;
    }
};

// 封装 POST 请求
export const post = async (path, body, options = {}) => {
    const url = baseURL + path;
    try {
        const response = await fetch(url, {
            method: "POST",
            timeout: 6000, //请求不能超过6S
            body: JSON.stringify(body),
            headers: {
                "Content-Type": "application/json",
                ...options.headers,
            },
            ...options,
        });
        if (response.status >= 200 && response.status < 300) {
            return await response.json();
        } else {
            throw new Error(`Request failed with status: ${response.status}`);
        }
    } catch (error) {
        console.error("POST request failed:", error);
        message.error("数据发送失败");
        throw error;
    }
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
