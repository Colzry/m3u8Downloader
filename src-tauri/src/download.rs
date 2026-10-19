//! M3U8 分片下载模块，支持AES-128加密流媒体解密
//! - 多线程并发下载
//! - 断点续传
//! - 自定请求头
//! - 支持任意伪装后缀（如 .pdf/.png 等）及伪装头自动剥离
//! - 支持 Data URI (Base64) 离线 M3U8 与自定义密钥

#![allow(deprecated)]
use crate::download_monitor::{run_monitor_task, DownloadMetrics};
use crate::merge::merge_files;
use aes::Aes128;
use anyhow::{anyhow, Result};
use cbc::Decryptor;
use cipher::generic_array::GenericArray;
use cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
use rand::{rngs::SmallRng, Rng, SeedableRng};
use reqwest::header::{HeaderName, HeaderValue};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{atomic::AtomicBool, atomic::Ordering, Arc};
use std::time::Duration;
use tauri::AppHandle;
use tokio::{
    fs,
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::{Mutex, Semaphore},
};

/// 加密信息结构体
/// 用于存储解密TS分片所需的密钥信息
#[derive(Clone, Serialize, Deserialize)]
struct EncryptionInfo {
    key: Vec<u8>,        // AES-128加密密钥（16字节）
    iv: Option<Vec<u8>>, // 初始化向量（16字节），None时使用分片媒体序列号作为IV
}

/// 十六进制字符串转字节向量
/// 示例：hex_to_bytes("0011ff") -> Ok(vec![0x00, 0x11, 0xff])
fn hex_to_bytes(s: &str) -> Result<Vec<u8>> {
    let s = s.trim();
    if s.len() % 2 != 0 {
        return Err(anyhow::anyhow!("Hex string has odd length"));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| anyhow::anyhow!("Invalid hex: {}", e))
        })
        .collect()
}

/// 内置轻量级 Base64 解码函数（免除外部 crate 依赖）
fn decode_base64(input: &str) -> Result<Vec<u8>> {
    let clean: Vec<u8> = input.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if clean.is_empty() {
        return Ok(Vec::new());
    }

    let mut output = Vec::with_capacity(clean.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0u8;

    for &b in &clean {
        if b == b'=' {
            break;
        }
        let val = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return Err(anyhow!("非法的 Base64 字符: {}", b as char)),
        };
        buf = (buf << 6) | (val as u32);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push(((buf >> bits) & 0xFF) as u8);
        }
    }
    Ok(output)
}

/// 解析 Data URI，提取解码后的二进制内容
/// 支持如: data:application/octet-stream;base64,I0VYVE0z...
fn parse_data_uri(uri: &str) -> Result<Vec<u8>> {
    let without_scheme = uri
        .strip_prefix("data:")
        .ok_or_else(|| anyhow!("不是有效的 Data URI"))?;
    let (meta, data) = without_scheme
        .split_once(',')
        .ok_or_else(|| anyhow!("Data URI 缺少逗号分隔符"))?;

    if meta.to_lowercase().contains(";base64") {
        decode_base64(data)
    } else {
        Ok(data.as_bytes().to_vec())
    }
}

/// 解析用户提供的外部密钥（支持 Hex、Base64、Data URI 或 16 字节原始字符）
fn parse_raw_key(raw: &str) -> Result<Vec<u8>> {
    let raw = raw.trim();
    if raw.starts_with("data:") {
        let bytes = parse_data_uri(raw)?;
        if bytes.len() == 16 {
            return Ok(bytes);
        }
        if let Ok(text) = std::str::from_utf8(&bytes) {
            return parse_raw_key(text);
        }
        return Err(anyhow!("Data URI 密钥长度不正确: {} 字节", bytes.len()));
    }

    // 1. 尝试 32 位 Hex 字符串（支持 0x 前缀）
    let hex_candidate = raw
        .strip_prefix("0x")
        .or_else(|| raw.strip_prefix("0X"))
        .unwrap_or(raw);
    if hex_candidate.len() == 32 && hex_candidate.chars().all(|c| c.is_ascii_hexdigit()) {
        return hex_to_bytes(hex_candidate);
    }

    // 2. 尝试 Base64 解码（16字节对应的 Base64 通常为 22~24 字符）
    if let Ok(bytes) = decode_base64(raw) {
        if bytes.len() == 16 {
            return Ok(bytes);
        }
    }

    // 3. 尝试直接作为 16 字节 ASCII 文本
    if raw.as_bytes().len() == 16 {
        return Ok(raw.as_bytes().to_vec());
    }

    Err(anyhow!(
        "无法解析密钥（需为 16 字节的 Base64、32 位 Hex 或 Data URI），当前输入: {}",
        raw
    ))
}

/// 解析M3U8的EXT-X-KEY标签（支持带引号且内部含逗号的 URI）
/// 返回元组：(加密方法, 密钥URI, IV值)
fn parse_ext_x_key(line: &str) -> Result<(String, String, Option<String>)> {
    let content = line.trim_start_matches("#EXT-X-KEY:").trim();
    let mut method = String::new();
    let mut uri = String::new();
    let mut iv = None;

    // 按引号外逗号安全分割键值对
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;

    for ch in content.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                current.push(ch);
            }
            ',' if !in_quotes => {
                parts.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        parts.push(current.trim().to_string());
    }

    for part in parts {
        if let Some((k, v)) = part.split_once('=') {
            let key = k.trim();
            let value = v.trim().trim_matches('"');
            match key {
                "METHOD" => method = value.to_string(),
                "URI" => uri = value.to_string(),
                "IV" => iv = Some(value.to_string()),
                _ => {}
            }
        }
    }
    Ok((method, uri, iv))
}

/// 将相对URL解析为绝对URL
/// - `base`: 当前M3U8文件的完整URL
/// - `relative`: 可能是绝对URL、Data URI、以 / 开头的根路径、或相对路径
fn resolve_url(base: &str, relative: &str) -> String {
    let relative = relative.trim();
    if relative.starts_with("http://")
        || relative.starts_with("https://")
        || relative.starts_with("data:")
    {
        relative.to_string()
    } else if relative.starts_with("//") {
        let scheme = base.split("://").next().unwrap_or("https");
        format!("{}:{}", scheme, relative)
    } else if relative.starts_with('/') {
        // 绝对路径 - 相对于域名根目录
        if base.starts_with("http://") || base.starts_with("https://") {
            let origin = base.split('/').take(3).collect::<Vec<&str>>().join("/");
            format!("{}{}", origin, relative)
        } else {
            relative.to_string()
        }
    } else {
        // 相对路径 - 相对于M3U8文件所在目录
        let base_without_query = base.split('?').next().unwrap_or(base);
        let dir = base_without_query
            .rsplit_once('/')
            .map(|(d, _)| d)
            .unwrap_or(base_without_query);
        if dir.is_empty() {
            relative.to_string()
        } else {
            format!("{}/{}", dir, relative)
        }
    }
}

/// 当 M3U8 来源是 Data URI 或离线文本时，尝试从分片绝对地址中提取主域名
fn infer_origin_from_m3u8(text: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("http://") || line.starts_with("https://") {
            let origin = line.split('/').take(3).collect::<Vec<&str>>().join("/");
            if origin.contains("://") {
                return Some(origin);
            }
        }
    }
    None
}

/// 检测并剥离伪造文件头（如 .pdf / .png 伪装头）
/// 返回：
/// - `Some(offset)`：找到有效 MPEG-TS 同步字节 0x47 的起始偏移量
/// - `None`：未找到有效 TS 结构（说明可能是 HTML/JSON 错误响应或解密密钥错误）
fn find_ts_sync_offset(data: &[u8]) -> Option<usize> {
    if data.is_empty() {
        return None;
    }
    // 标准 TS 包大小为 188 字节
    if data.len() < 188 {
        return if data[0] == 0x47 { Some(0) } else { None };
    }

    // 限制最大扫描范围（前 8KB），足以跳过常见的 PNG/PDF/JPG 伪造头
    let max_search = data.len().saturating_sub(188).min(8192);
    for offset in 0..=max_search {
        if data[offset] == 0x47 && data[offset + 188] == 0x47 {
            // 若数据足够长，验证第 3 个同步点以彻底排除巧合
            if offset + 376 < data.len() {
                if data[offset + 376] == 0x47 {
                    return Some(offset);
                }
            } else {
                return Some(offset);
            }
        }
    }
    None
}

/// 自定义下载选项
#[derive(Debug, Clone)]
pub struct DownloadOptions {
    pub headers: HashMap<String, String>,
    pub custom_key: Option<String>, // 可选：外部直接指定的 AES-128 密钥（支持 Base64 / Hex / Data URI）
    pub base_url: Option<String>, // 可选：当 M3U8 为 Data URI / 本地文本时用于解析相对路径的基准 URL
}

impl DownloadOptions {
    pub fn new() -> Self {
        Self {
            headers: HashMap::new(),
            custom_key: None,
            base_url: None,
        }
    }
}

pub enum DownloadResult {
    Success(String),   // 成功并且是有效 ts 文件
    Skipped(String),   // 下载成功，但内容无效或空，未写入磁盘
    Cancelled(String), // 因用户取消而中断下载
}

/// 自定义下载请求头（自动过滤内部约定的特殊配置头，如 X-M3U8-Key / X-Base-Url）
fn preprocess_headers(headers: &HashMap<String, String>) -> reqwest::header::HeaderMap {
    let mut valid_headers = reqwest::header::HeaderMap::new();
    for (key, value) in headers {
        let key_lower = key.trim().to_lowercase();
        if key_lower == "x-m3u8-key" || key_lower == "x-custom-key" || key_lower == "x-base-url" {
            continue;
        }
        match (
            HeaderName::from_bytes(key.as_bytes()),
            HeaderValue::from_str(value),
        ) {
            (Ok(header_name), Ok(header_value)) => {
                valid_headers.insert(header_name, header_value);
            }
            (Err(_), _) => {
                log::warn!("无效的请求头名称，已跳过: {}", key);
            }
            (_, Err(_)) => {
                log::warn!("无效的请求头值，已跳过: {}={}", key, value);
            }
        }
    }
    valid_headers
}

/// 从 options 或特殊请求头中提取自定义密钥与基准 URL
fn extract_special_options(options: &DownloadOptions) -> (Option<String>, Option<String>) {
    let mut custom_key = options.custom_key.clone().filter(|s| !s.trim().is_empty());
    let mut base_url = options.base_url.clone().filter(|s| !s.trim().is_empty());

    for (k, v) in &options.headers {
        let k_lower = k.trim().to_lowercase();
        if custom_key.is_none() && (k_lower == "x-m3u8-key" || k_lower == "x-custom-key") {
            if !v.trim().is_empty() {
                custom_key = Some(v.trim().to_string());
            }
        }
        if base_url.is_none() && k_lower == "x-base-url" {
            if !v.trim().is_empty() {
                base_url = Some(v.trim().to_string());
            }
        }
    }

    (custom_key, base_url)
}

/// 获取并验证 16 字节 AES-128 密钥
async fn fetch_encryption_key(
    client: &Client,
    key_url: &str,
    headers: &reqwest::header::HeaderMap,
    custom_key: Option<&str>,
) -> Result<Vec<u8>> {
    // 1. 若用户显式指定了外部密钥，优先使用
    if let Some(raw_key) = custom_key {
        log::info!("使用外部自定义密钥进行解密");
        return parse_raw_key(raw_key);
    }

    // 2. 若 M3U8 内部 URI 本身是 Data URI
    if key_url.starts_with("data:") {
        return parse_raw_key(key_url);
    }

    // 3. 校验 URL 是否合法
    if !(key_url.starts_with("http://") || key_url.starts_with("https://")) {
        return Err(anyhow!(
            "密钥地址是相对路径 ({}) 且无法推断主域名，请提供完整 M3U8 URL、base_url 或直接提供 custom_key",
            key_url
        ));
    }

    // 4. 发送 HTTP 请求获取密钥
    let resp = client.get(key_url).headers(headers.clone()).send().await?;
    if !resp.status().is_success() {
        return Err(anyhow!(
            "获取密钥失败 ({}): HTTP {}",
            key_url,
            resp.status()
        ));
    }
    let key_bytes = resp.bytes().await?.to_vec();

    // 标准 16 字节二进制密钥
    if key_bytes.len() == 16 {
        return Ok(key_bytes);
    }

    // 兼容部分接口返回 Base64 或 Hex 文本格式的密钥
    if let Ok(text) = std::str::from_utf8(&key_bytes) {
        if let Ok(parsed) = parse_raw_key(text) {
            return Ok(parsed);
        }
    }

    Err(anyhow!(
        "从 {} 获取的密钥长度无效（期望 16 字节，实际 {} 字节）",
        key_url,
        key_bytes.len()
    ))
}

/// 下载单个TS文件（支持加密内容解密与伪装头剥离）
async fn download_file(
    seq_num: u64, // 当前分片的 Media Sequence Number，用于计算默认 IV
    client: &Client,
    url: &str,
    output_path: &str,
    cancelled: &Arc<AtomicBool>,
    encryption: Option<EncryptionInfo>,
    metrics: Arc<DownloadMetrics>,        // metrics参数
    headers: &reqwest::header::HeaderMap, // 预处理后的有效请求头
) -> Result<DownloadResult> {
    // 构建带自定义请求头的请求
    let request = client.get(url).headers(headers.clone());

    let mut response = request.send().await?;
    let status = response.status();
    if !status.is_success() {
        return Err(anyhow!(
            "分片请求失败 [{}], 状态码: {}",
            url,
            status.as_u16()
        ));
    }

    // 检查是否 HTML/XML 错误页（基于 Content-Type 头，注意不要拦截 application/pdf 或 image/* 伪装头）
    let content_type = response
        .headers()
        .get("Content-Type")
        .and_then(|ct| ct.to_str().ok())
        .unwrap_or("")
        .to_lowercase();

    if content_type.starts_with("text/html") || content_type.contains("xml") {
        log::warn!(
            "[{}] Content-Type 为 HTML/XML ({}), 视为网络错误以便重试",
            url,
            content_type
        );
        return Err(anyhow!(
            "服务器返回了 HTML/XML 内容而非 TS 分片，Content-Type: {}",
            content_type
        ));
    }

    let mut buffer = Vec::new();

    while let Some(chunk) = response.chunk().await? {
        // 每次下载数据块后立即检查取消
        if cancelled.load(Ordering::Relaxed) {
            // 主动清理已下载的部分文件
            fs::remove_file(output_path).await.ok();
            return Ok(DownloadResult::Cancelled(url.to_string()));
        }

        // 记录下载数据
        let chunk_len = chunk.len();
        buffer.extend_from_slice(&chunk);
        metrics.record_chunk(chunk_len).await;
    }

    // 判断是否为空
    if buffer.is_empty() {
        log::warn!("[{}] 返回空数据，标记为 Skipped", url);
        return Ok(DownloadResult::Skipped(url.to_string()));
    }

    // AES-128解密处理
    // 注意：不能在解密前检查首字节是否为 '<' 或 '{'，因为加密密文是随机字节，有概率碰撞
    let data: Vec<u8> = if let Some(enc) = encryption {
        // HLS标准：如果IV为空，则使用分片的Media Sequence Number作为大端序16字节IV
        let iv_vec = enc.iv.unwrap_or_else(|| {
            let mut iv = vec![0u8; 16];
            let seq_bytes = seq_num.to_be_bytes();
            iv[8..16].copy_from_slice(&seq_bytes);
            iv
        });

        if enc.key.len() != 16 || iv_vec.len() != 16 {
            return Err(anyhow!(
                "无效的 AES-128 密钥或 IV 长度 (key={}, iv={})",
                enc.key.len(),
                iv_vec.len()
            ));
        }

        let key = GenericArray::from_slice(&enc.key);
        let iv = GenericArray::from_slice(&iv_vec);

        // 兼容两类伪装方案：
        // 方案 A：服务端直接对标准 TS（或带伪装头的 TS）整体做 AES-128-CBC 加密
        // 方案 B：服务端先对 TS 做 AES 加密，随后在密文头部拼接了一段非 16 字节对齐的伪装头
        let mut buf = buffer;
        let rem = buf.len() % 16;
        if rem != 0 && buf.len() > 16 {
            log::debug!(
                "分片 [{}] 密文长度 ({}) 非 16 字节对齐，尝试剥离前 {} 字节混淆头",
                url,
                buf.len(),
                rem
            );
            buf.drain(..rem);
        }

        let decryptor = Decryptor::<Aes128>::new(key, iv);
        let decrypted = decryptor
            .decrypt_padded_mut::<Pkcs7>(&mut buf)
            .map_err(|e| anyhow!("AES-128 解密失败: {:?}", e))?;

        decrypted.to_vec()
    } else {
        buffer
    };

    // 解密后（或明文状态）校验内容并自动剥离伪装文件头（如 %PDF- 或 PNG 头部）
    let clean_slice = match find_ts_sync_offset(&data) {
        Some(0) => &data[..],
        Some(offset) => {
            log::debug!(
                "分片 [{}] 检测到 {} 字节头部伪装偏移，已自动剥离",
                url,
                offset
            );
            &data[offset..]
        }
        None => {
            // 未找到 0x47 同步字节，检查是否是明文 HTML / JSON 错误响应
            if data.len() >= 4 {
                let first_byte = data[0];
                if first_byte == b'<' {
                    return Err(anyhow!("服务器返回了 HTML 内容而非 TS 分片"));
                }
                if first_byte == b'{' || first_byte == b'[' {
                    return Err(anyhow!("服务器返回了 JSON 内容而非 TS 分片"));
                }
            }
            log::warn!(
                "分片 [{}] 解密后未检测到标准 0x47 同步字节（可能是 fMP4 分片或特殊封装），按原样写入",
                url
            );
            &data[..]
        }
    };

    // 写入解密并剥离伪造头后的文件
    let mut file = fs::File::create(output_path).await?;
    file.write_all(clean_slice).await?;
    Ok(DownloadResult::Success(output_path.to_string()))
}

/// 分片信息结构（支持向后兼容的反序列化）
#[derive(Serialize, Deserialize)]
struct SegmentMetadata {
    #[serde(default)]
    seq_num: Option<u64>,
    url: String,
    local_path: String,
    encryption: Option<EncryptionInfo>,
}

async fn validate_m3u8_response(
    status: StatusCode,
    text: &str,
    content_type: Option<&str>,
) -> Result<()> {
    // 状态码验证
    if !status.is_success() {
        return Err(match status.as_u16() {
            403 => anyhow::anyhow!("403 Forbidden：服务器拒绝访问，可能需要添加请求头"),
            404 => anyhow::anyhow!("404 Not Found：地址无效或文件不存在"),
            code => anyhow::anyhow!("请求失败，状态码：{}", code),
        });
    }

    // Content-Type 验证
    if let Some(ct) = content_type {
        let ct_lower = ct.to_lowercase();
        if !(ct_lower.contains("mpegurl")
            || ct_lower.contains("m3u8")
            || ct_lower.contains("plain")
            || ct_lower.contains("text")
            || ct_lower.contains("application/octet-stream")
            || ct_lower.contains("binary"))
        {
            // 若 Content-Type 非标准，但正文确实以 #EXTM3U 开头，也予以放行
            if !text.trim_start().starts_with("#EXTM3U") {
                return Err(anyhow::anyhow!("Content-Type 不匹配 M3U8 文件：{}", ct));
            }
        }
    }

    // 内容验证
    if !text.trim_start().starts_with("#EXTM3U") {
        return Err(anyhow::anyhow!("M3U8 内容无效：缺少 #EXTM3U 标识"));
    }

    Ok(())
}

/// 获取 M3U8 文本及解析基准 URL（支持 http/https、data: Base64、以及直接传入 #EXTM3U 文本）
async fn fetch_m3u8_content(
    client: &Client,
    url: &str,
    headers: &reqwest::header::HeaderMap,
    fallback_base_url: Option<&str>,
) -> Result<(String, String)> {
    let trimmed = url.trim();

    // 1. 支持 Data URI (如 data:application/octet-stream;base64,...)
    if trimmed.starts_with("data:") {
        let bytes = parse_data_uri(trimmed)?;
        let text = String::from_utf8(bytes)
            .map_err(|e| anyhow!("Data URI 解码后不是有效的 UTF-8 文本: {}", e))?;
        validate_m3u8_response(StatusCode::OK, &text, Some("application/x-mpegURL")).await?;
        let base = fallback_base_url
            .map(|s| s.to_string())
            .or_else(|| infer_origin_from_m3u8(&text))
            .unwrap_or_default();
        return Ok((text, base));
    }

    // 2. 支持直接传入原始 #EXTM3U 文本内容
    if trimmed.starts_with("#EXTM3U") {
        validate_m3u8_response(StatusCode::OK, trimmed, Some("application/x-mpegURL")).await?;
        let base = fallback_base_url
            .map(|s| s.to_string())
            .or_else(|| infer_origin_from_m3u8(trimmed))
            .unwrap_or_default();
        return Ok((trimmed.to_string(), base));
    }

    // 3. 标准 HTTP/HTTPS 网络请求
    let request = client.get(trimmed).headers(headers.clone());
    let raw_response = request.send().await?;
    let status = raw_response.status();
    let content_type = raw_response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let response_text = raw_response.text().await?;

    validate_m3u8_response(status, &response_text, content_type.as_deref()).await?;
    Ok((response_text, trimmed.to_string()))
}

/// M3U8下载主函数
pub async fn download_m3u8(
    id: String,                 // 下载任务唯一标识
    url: &str,                  // M3U8文件URL（或 Data URI）
    name: &str,                 // 输出文件名
    temp_dir: &str,             // ts文件下载目录
    output_dir: &str,           // MP4视频输出目录
    concurrency: usize,         // 并发线程数
    cancelled: Arc<AtomicBool>, // 取消标志
    app_handle: AppHandle,      // Tauri应用句柄
    options: DownloadOptions,   // 下载选项（包含自定义headers、custom_key等）
) -> Result<()> {
    // 创建输出目录
    fs::create_dir_all(temp_dir).await?;

    let client = Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(60))
        .pool_max_idle_per_host(concurrency)
        .build()
        .map_err(|e| anyhow!("创建 HTTP Client 失败: {}", e))?;

    // 提取特殊配置并预处理常规请求头
    let (custom_key, fallback_base_url) = extract_special_options(&options);
    let headers = preprocess_headers(&options.headers);
    log::info!("headers: {:#?}", headers);

    // --- 步骤 1: 解析M3U8，收集所有分片信息 ---
    // 分片元数据文件路径
    let segments_metadata_path = format!("{}/segments.json", temp_dir);
    // 元组结构: (seq_num, ts_url, local_path, encryption)
    let mut all_ts_segments: Vec<(u64, String, String, Option<EncryptionInfo>)> = Vec::new();

    // 尝试从保存的元数据文件中加载分片信息
    if tokio::fs::metadata(&segments_metadata_path).await.is_ok() {
        log::info!("从本地加载分片元数据: {}", segments_metadata_path);
        let metadata_content = tokio::fs::read_to_string(&segments_metadata_path).await?;
        let segments_metadata: Vec<SegmentMetadata> = serde_json::from_str(&metadata_content)?;

        for (index, segment) in segments_metadata.into_iter().enumerate() {
            let seq_num = segment.seq_num.unwrap_or(index as u64);
            all_ts_segments.push((seq_num, segment.url, segment.local_path, segment.encryption));
        }
    } else {
        // 第一次下载，获取并解析M3U8内容
        let (initial_text, initial_base_url) =
            fetch_m3u8_content(&client, url, &headers, fallback_base_url.as_deref()).await?;

        // 检测是否为 Master Playlist（包含 #EXT-X-STREAM-INF）
        // 如果是，选择最高码率的子流继续解析
        let (response_text, parse_base_url) = if initial_text
            .lines()
            .any(|l| l.trim().starts_with("#EXT-X-STREAM-INF"))
        {
            log::info!("检测到 Master Playlist，正在选择最高码率子流...");
            let mut best_bandwidth = 0u64;
            let mut best_url = String::new();
            let mut pending_bandwidth = 0u64;
            let mut expect_sub_stream = false;

            for line in initial_text.lines() {
                let line = line.trim();
                if line.starts_with("#EXT-X-STREAM-INF") {
                    pending_bandwidth = line
                        .split(',')
                        .find(|p| p.trim().starts_with("BANDWIDTH="))
                        .and_then(|p| p.trim().strip_prefix("BANDWIDTH="))
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(1); // 若未标明带宽，默认给 1 确保能选中
                    expect_sub_stream = true;
                } else if !line.is_empty() && !line.starts_with('#') && expect_sub_stream {
                    if pending_bandwidth >= best_bandwidth {
                        best_bandwidth = pending_bandwidth;
                        best_url = line.to_string();
                    }
                    pending_bandwidth = 0;
                    expect_sub_stream = false;
                }
            }

            if best_url.is_empty() {
                return Err(anyhow!("Master Playlist 中未找到有效的子流"));
            }

            let sub_url = resolve_url(&initial_base_url, &best_url);
            log::info!("已选择子流 (bandwidth={}): {}", best_bandwidth, sub_url);

            fetch_m3u8_content(&client, &sub_url, &headers, fallback_base_url.as_deref()).await?
        } else {
            (initial_text, initial_base_url)
        };

        let mut current_encryption: Option<EncryptionInfo> = None;
        let mut key_cache: HashMap<String, Vec<u8>> = HashMap::new();
        let mut media_sequence: u64 = 0;
        let mut ts_index: usize = 0;
        let mut expect_segment_url = false;

        for line in response_text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            // 解析起始媒体序列号 #EXT-X-MEDIA-SEQUENCE
            if let Some(seq_str) = line.strip_prefix("#EXT-X-MEDIA-SEQUENCE:") {
                if let Ok(seq) = seq_str.trim().parse::<u64>() {
                    media_sequence = seq;
                }
                continue;
            }

            // 解析加密信息 #EXT-X-KEY
            if line.starts_with("#EXT-X-KEY:") {
                let (method, key_uri, iv_str) = parse_ext_x_key(line)?;
                if method.to_uppercase() == "AES-128" {
                    let key_url = resolve_url(&parse_base_url, &key_uri);

                    let key = if let Some(cached) = key_cache.get(&key_url) {
                        cached.clone()
                    } else {
                        let fetched = fetch_encryption_key(
                            &client,
                            &key_url,
                            &headers,
                            custom_key.as_deref(),
                        )
                        .await?;
                        key_cache.insert(key_url.clone(), fetched.clone());
                        fetched
                    };

                    let iv = iv_str.as_ref().and_then(|iv_raw| {
                        let hex = iv_raw
                            .strip_prefix("0x")
                            .or_else(|| iv_raw.strip_prefix("0X"))
                            .unwrap_or(iv_raw);
                        hex_to_bytes(hex).ok()
                    });

                    current_encryption = Some(EncryptionInfo { key, iv });
                } else if method.to_uppercase() == "NONE" {
                    current_encryption = None;
                } else {
                    return Err(anyhow!("暂不支持的加密算法: {}", method));
                }
                continue;
            }

            // 标记下一行非注释行为有效媒体分片
            if line.starts_with("#EXTINF:") {
                expect_segment_url = true;
                continue;
            }

            // 收集分片任务：支持 #EXTINF 引导的任意后缀（如 .pdf/.png/.jpg/无后缀），同时向后兼容直接写 .ts 的列表
            if !line.starts_with('#') {
                if expect_segment_url || line.contains(".ts") || line.contains(".m4s") {
                    let ts_url = resolve_url(&parse_base_url, line);
                    // 本地统一存为 part_{index}.ts，供后续 FFmpeg concat 正常合并
                    let filename = format!("{}/part_{}.ts", temp_dir, ts_index);
                    let seq_num = media_sequence + (ts_index as u64);
                    all_ts_segments.push((seq_num, ts_url, filename, current_encryption.clone()));
                    ts_index += 1;
                    expect_segment_url = false;
                }
            }
        }

        // 保存分片元数据到文件，供后续断点续传使用
        let segments_metadata: Vec<SegmentMetadata> = all_ts_segments
            .iter()
            .map(|(seq_num, url, local_path, encryption)| SegmentMetadata {
                seq_num: Some(*seq_num),
                url: url.clone(),
                local_path: local_path.clone(),
                encryption: encryption.clone(),
            })
            .collect();

        let metadata_json = serde_json::to_string(&segments_metadata)?;
        tokio::fs::write(&segments_metadata_path, metadata_json).await?;
        log::info!("已保存分片元数据到: {}", segments_metadata_path);
    }

    if all_ts_segments.is_empty() {
        log::warn!("M3U8 [{} {}] 中未找到任何有效媒体分片", id, name);
        return Err(anyhow::anyhow!("M3U8中未找到任何有效媒体分片"));
    }

    // --- 步骤 2: 断点续传检查 (基于 Manifest 文件) ---
    let total_chunks = all_ts_segments.len();
    let metrics = Arc::new(DownloadMetrics::new(total_chunks));

    // 从 M3U8 解析列表按顺序构建最终合并的文件路径列表
    let final_ts_files: Vec<String> = all_ts_segments
        .iter()
        .map(|(_, _, path, _)| path.clone())
        .collect();

    let manifest_path = format!("{}/progress.dat", temp_dir);

    // --- 步骤 3: 启动速度监控任务（仅启动一次，贯穿所有重试轮次）---
    let speed_handle = run_monitor_task(
        id.clone(),
        Arc::clone(&cancelled),
        Arc::clone(&metrics),
        app_handle.clone(),
    )
    .await;

    // ==========================================
    //  总体下载重试循环
    //  当分片未集齐时，自动重新评估缺失分片并重试下载
    // ==========================================
    const MAX_OVERALL_RETRIES: usize = 5;
    let mut overall_retries = 0;
    let mut is_first_pass = true;

    loop {
        // 存储真正需要下载的任务
        let mut pending_downloads = Vec::new();

        // 加载清单文件，重新评估已完成的分片
        let mut completed_segment_names = HashSet::new();
        if let Ok(file) = tokio::fs::File::open(&manifest_path).await {
            let reader = BufReader::new(file);
            let mut lines = reader.lines();
            while let Some(line) = lines.next_line().await? {
                if !line.trim().is_empty() {
                    completed_segment_names.insert(line);
                }
            }
        }
        log::info!(
            "任务 [{}]: 从清单文件中加载了 {} 条已完成记录 (第 {} 轮)",
            id,
            completed_segment_names.len(),
            overall_retries + 1
        );

        for (seq_num, ts_url, filename, encryption) in &all_ts_segments {
            let relative_name = match Path::new(&filename).file_name().and_then(|s| s.to_str()) {
                Some(name) => name.to_string(),
                None => continue,
            };

            if completed_segment_names.contains(&relative_name) {
                match tokio::fs::metadata(&filename).await {
                    Ok(metadata) if metadata.len() > 0 => {
                        // 仅首轮更新进度计数器，避免重试轮次重复计数
                        if is_first_pass {
                            let file_size = metadata.len() as usize;
                            metrics.completed_chunks.fetch_add(1, Ordering::Relaxed);
                            metrics
                                .downloaded_bytes
                                .fetch_add(file_size, Ordering::Relaxed);
                            metrics.update_total_bytes(file_size);
                        }
                    }
                    _ => {
                        // 清单有记录但文件丢失/为空 → 重新下载
                        pending_downloads.push((
                            *seq_num,
                            ts_url.clone(),
                            filename.clone(),
                            encryption.clone(),
                        ));
                    }
                }
            } else {
                // 清单无记录 → 需要下载
                pending_downloads.push((
                    *seq_num,
                    ts_url.clone(),
                    filename.clone(),
                    encryption.clone(),
                ));
            }
        }
        is_first_pass = false;

        log::info!(
            "任务 [{}]: 总分片 {}, 清单已完成 {}, 本轮待下载 {} (第 {} 轮)",
            id,
            total_chunks,
            completed_segment_names.len(),
            pending_downloads.len(),
            overall_retries + 1
        );

        // 如果本轮没有待下载分片，说明全部集齐，退出循环
        if pending_downloads.is_empty() {
            log::info!("任务 [{}] 所有分片均已就绪，准备合并", id);
            break;
        }

        // --- 步骤 4: 启动下载任务 (只下载 pending_downloads) ---
        let manifest_writer = Arc::new(Mutex::new(
            tokio::fs::File::options()
                .append(true)
                .create(true)
                .open(&manifest_path)
                .await?,
        ));

        let semaphore = Arc::new(Semaphore::new(concurrency));
        let mut handles = Vec::new();

        for (seq_num, ts_url, filename, encryption) in pending_downloads {
            let client = client.clone();
            let semaphore = Arc::clone(&semaphore);
            let cancelled = Arc::clone(&cancelled);
            let metrics = Arc::clone(&metrics);
            let manifest_writer = Arc::clone(&manifest_writer);
            let headers = headers.clone();

            handles.push(tokio::spawn(async move {
                let _permit = semaphore.acquire().await?;

                const MAX_RETRIES: usize = 15;
                for attempt in 1..=MAX_RETRIES {
                    if cancelled.load(Ordering::Relaxed) {
                        return Ok::<(), anyhow::Error>(());
                    }
                    let result = download_file(
                        seq_num,
                        &client,
                        &ts_url,
                        &filename,
                        &cancelled,
                        encryption.clone(),
                        metrics.clone(),
                        &headers,
                    )
                    .await;

                    match result {
                        Ok(DownloadResult::Success(f)) => {
                            log::debug!("分片 [{}] 下载成功（尝试次数 {}）", f, attempt);

                            if let Some(relative_name) =
                                Path::new(&f).file_name().and_then(|s| s.to_str())
                            {
                                let mut writer = manifest_writer.lock().await;
                                writer
                                    .write_all(format!("{}\n", relative_name).as_bytes())
                                    .await?;
                                writer.flush().await?;
                            }

                            // 将已完成计数器 +1
                            metrics.completed_chunks.fetch_add(1, Ordering::Relaxed);
                            return Ok(());
                        }
                        Ok(DownloadResult::Skipped(f)) => {
                            log::warn!(
                                "分片 [{}] 返回空数据（尝试 {}/{}），稍后重试",
                                f,
                                attempt,
                                MAX_RETRIES
                            );
                            if attempt < MAX_RETRIES {
                                let base_delay_secs = (1 << (attempt - 1)).min(10);
                                let mut rng = SmallRng::from_entropy();
                                let random_millis = rng.gen_range(0..1000);
                                let total_delay = Duration::from_secs(base_delay_secs as u64)
                                    + Duration::from_millis(random_millis);
                                log::info!("分片 [{}] 空数据退避，等待 {:?}", f, total_delay);
                                tokio::time::sleep(total_delay).await;
                            } else {
                                log::error!("分片 [{}] 所有重试均返回空数据，放弃该分片", f);
                                return Ok(());
                            }
                        }
                        Ok(DownloadResult::Cancelled(f)) => {
                            log::debug!("分片 [{}] 因取消而中断", f);
                            return Ok(());
                        }
                        Err(e) => {
                            log::warn!(
                                "分片 [{}] 第 {} 次下载失败，原因：{}",
                                filename,
                                attempt,
                                e
                            );
                            if attempt < MAX_RETRIES {
                                let base_delay_secs = (1 << (attempt - 1)).min(10);
                                let mut rng = SmallRng::from_entropy();
                                let random_millis = rng.gen_range(0..1000);
                                let total_delay = Duration::from_secs(base_delay_secs as u64)
                                    + Duration::from_millis(random_millis);

                                log::info!("分片 [{}] 正在退避，等待 {:?}", filename, total_delay);
                                tokio::time::sleep(total_delay).await;
                            } else {
                                log::error!(
                                    "分片 [{}] 所有重试失败: {:?}, 将在下一轮重试",
                                    filename,
                                    e
                                );
                            }
                        }
                    }
                }
                Ok(())
            }));
        }

        // --- 步骤 5: 等待本轮所有下载任务完成 ---
        for handle in handles {
            match handle.await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => log::warn!("分片下载任务异常退出: {}", e),
                Err(e) => log::error!("分片下载任务崩溃: {}", e),
            }
        }

        // 从清单文件统计实际完成数（自动去重）
        let completed_count = {
            let mut unique_names = HashSet::new();
            if let Ok(file) = tokio::fs::File::open(&manifest_path).await {
                let reader = BufReader::new(file);
                let mut lines = reader.lines();
                while let Some(line) = lines.next_line().await? {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        unique_names.insert(trimmed.to_string());
                    }
                }
            }
            unique_names.len()
        };

        // 用户主动取消 → 退出循环
        if cancelled.load(Ordering::Relaxed) {
            log::info!(
                "任务 [{}] 未完成下载。预期: {}, 已完成: {}. 任务已被取消",
                id,
                total_chunks,
                completed_count
            );
            break;
        }

        // 所有分片已集齐 → 退出循环
        if completed_count >= total_chunks {
            log::info!("任务 [{}] 所有分片均已就绪，准备合并", id);
            break;
        }

        // 未集齐 → 检查是否达到最大重试次数
        overall_retries += 1;
        if overall_retries >= MAX_OVERALL_RETRIES {
            log::error!(
                "任务 [{}] 经过 {} 轮尝试仍未能集齐所有分片。预期: {}, 实际: {}. 下载失败",
                id,
                MAX_OVERALL_RETRIES,
                total_chunks,
                completed_count
            );
            cancelled.store(true, Ordering::SeqCst);
            break;
        }

        log::warn!(
            "任务 [{}] 第 {} 轮未能集齐所有分片 (已集齐 {}/{})，3秒后开始第 {} 轮重试...",
            id,
            overall_retries,
            completed_count,
            total_chunks,
            overall_retries + 1
        );
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }

    // 等待速度监控任务退出
    speed_handle.await?;

    // 最终检查：如果被取消，返回 Ok（由上层处理）
    if cancelled.load(Ordering::Relaxed) {
        log::warn!("任务 [{}] 检测已被取消，结束下载", id);
        return Ok(());
    }

    // --- 步骤 6: 合并 TS 文件为 MP4 ---
    merge_files(
        id.clone(),
        &name,
        final_ts_files,
        &temp_dir,
        &output_dir,
        app_handle.clone(),
    )
    .await?;

    Ok(())
}
