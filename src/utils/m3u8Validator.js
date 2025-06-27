export const M3U8_VALID_CONTENT_TYPES = [
  'application/vnd.apple.mpegurl',
  'application/x-mpegurl',
  'audio/mpegurl',
  'application/octet-stream',
  'text/plain'
];

export const validateM3u8Url = async (url, options = {}) => {
  const defaultOptions = {
    checkContent: true,
    timeout: 3000,
    ...options
  };
  
  const resultTemplate = {
    valid: false,
    message: '',
    details: {
      url: url,
      statusCode: null,
      contentType: null,
      contentLength: null,
      contentValid: false
    }
  };
  
  try {
    // 基础格式校验
    if (!/^https?:\/\//i.test(url)) {
      throw new Error('URL必须以http或https开头');
    }
    
    // 创建AbortController用于超时控制
    const controller = new AbortController();
    const timeoutId = setTimeout(() => {
      controller.abort();
      throw new Error(`请求超时（${defaultOptions.timeout}ms）`);
    }, defaultOptions.timeout);
    
    // HEAD请求验证
    const headResponse = await fetch(url, {
      method: 'HEAD',
      signal: controller.signal
    });
    clearTimeout(timeoutId);
    
    resultTemplate.details.statusCode = headResponse.status;
    resultTemplate.details.contentType = headResponse.headers.get('Content-Type');
    resultTemplate.details.contentLength = headResponse.headers.get('Content-Length');
    
    if (!headResponse.ok) {
      throw new Error(`服务器返回状态：${headResponse.status}`);
    }
    
    // Content-Type验证
    const receivedType = (resultTemplate.details.contentType || '').toLowerCase();
    const isValidType = M3U8_VALID_CONTENT_TYPES.some(validType =>
      receivedType.includes(validType.toLowerCase())
    );
    if (!isValidType) {
      throw new Error('Content-Type不符合M3U8格式要求');
    }
    
    // 内容验证（可选）
    if (defaultOptions.checkContent) {
      const getResponse = await fetch(url, { signal: controller.signal });
      const text = await getResponse.text();
      resultTemplate.details.contentValid = text.startsWith('#EXTM3U');
      
      if (!resultTemplate.details.contentValid) {
        throw new Error('文件内容缺少M3U8标识');
      }
    }
    
    return {
      ...resultTemplate,
      valid: true,
      message: '有效的M3U8地址'
    };
  } catch (error) {
    return {
      ...resultTemplate,
      message: error.message || '验证过程中发生未知错误'
    };
  }
};