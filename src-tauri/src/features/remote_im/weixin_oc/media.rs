fn build_weixin_oc_http_client(timeout_ms: u64) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(timeout_ms))
        .build()
        .map_err(|err| format!("创建个人微信 HTTP 客户端失败: {err}"))
}

fn weixin_oc_cdn_download_url(encrypted_query_param: &str) -> String {
    format!(
        "{}/download?encrypted_query_param={}",
        WEIXIN_OC_DEFAULT_CDN_BASE_URL.trim_end_matches('/'),
        urlencoding::encode(encrypted_query_param.trim())
    )
}

fn weixin_oc_pkcs7_unpad(data: &[u8]) -> Vec<u8> {
    let Some(&pad_len) = data.last() else {
        return Vec::new();
    };
    let pad_len = pad_len as usize;
    if pad_len == 0 || pad_len > 16 || pad_len > data.len() {
        return data.to_vec();
    }
    if data[data.len() - pad_len..]
        .iter()
        .all(|value| *value as usize == pad_len)
    {
        data[..data.len() - pad_len].to_vec()
    } else {
        data.to_vec()
    }
}

fn weixin_oc_decode_hex(input: &str) -> Result<Vec<u8>, String> {
    let normalized = input.trim();
    if normalized.is_empty() {
        return Err("十六进制密钥为空".to_string());
    }
    if normalized.len() % 2 != 0 {
        return Err("十六进制密钥长度不正确".to_string());
    }
    let mut out = Vec::with_capacity(normalized.len() / 2);
    let bytes = normalized.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() {
        let hi = (bytes[idx] as char)
            .to_digit(16)
            .ok_or_else(|| "十六进制密钥包含非法字符".to_string())?;
        let lo = (bytes[idx + 1] as char)
            .to_digit(16)
            .ok_or_else(|| "十六进制密钥包含非法字符".to_string())?;
        out.push(((hi << 4) | lo) as u8);
        idx += 2;
    }
    Ok(out)
}

fn weixin_oc_parse_media_aes_key(aes_key_value: &str) -> Result<Vec<u8>, String> {
    let normalized = aes_key_value.trim();
    if normalized.is_empty() {
        return Err("媒体 AES 密钥为空".to_string());
    }
    let padded = format!(
        "{}{}",
        normalized,
        "=".repeat((4usize.wrapping_sub(normalized.len() % 4)) % 4)
    );
    let decoded = B64
        .decode(padded.as_bytes())
        .map_err(|err| format!("解析媒体 AES 密钥失败: {err}"))?;
    if decoded.len() == 16 {
        return Ok(decoded);
    }
    if decoded.len() == 32
        && decoded
            .iter()
            .all(|byte| (*byte as char).is_ascii_hexdigit())
    {
        let hex_text =
            std::str::from_utf8(&decoded).map_err(|err| format!("解析媒体 AES 十六进制失败: {err}"))?;
        return weixin_oc_decode_hex(hex_text);
    }
    Err("媒体 AES 密钥格式不支持".to_string())
}

fn weixin_oc_decrypt_media_ecb(encrypted: &[u8], key: &[u8]) -> Result<Vec<u8>, String> {
    use aes::cipher::{generic_array::GenericArray, BlockDecrypt, KeyInit};

    if key.len() != 16 {
        return Err(format!("媒体 AES 密钥长度不正确: {}", key.len()));
    }
    if encrypted.is_empty() {
        return Ok(Vec::new());
    }
    if encrypted.len() % 16 != 0 {
        return Err(format!("媒体密文长度不是 16 的倍数: {}", encrypted.len()));
    }
    let cipher = aes::Aes128::new_from_slice(key)
        .map_err(|err| format!("初始化媒体 AES 解密器失败: {err}"))?;
    let mut decrypted = encrypted.to_vec();
    for chunk in decrypted.chunks_exact_mut(16) {
        let block = GenericArray::from_mut_slice(chunk);
        cipher.decrypt_block(block);
    }
    Ok(weixin_oc_pkcs7_unpad(&decrypted))
}

async fn weixin_oc_download_image_bytes(
    client: &reqwest::Client,
    encrypted_query_param: &str,
    aes_key_value: Option<&str>,
) -> Result<Vec<u8>, String> {
    let resp = client
        .get(weixin_oc_cdn_download_url(encrypted_query_param))
        .send()
        .await
        .map_err(|err| format!("下载个人微信图片失败: {err}"))?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("下载个人微信图片失败: status={} body={}", status, body));
    }
    let encrypted = resp
        .bytes()
        .await
        .map_err(|err| format!("读取个人微信图片响应失败: {err}"))?;
    if let Some(value) = aes_key_value.map(str::trim).filter(|value| !value.is_empty()) {
        let key = weixin_oc_parse_media_aes_key(value)?;
        return weixin_oc_decrypt_media_ecb(encrypted.as_ref(), &key);
    }
    Ok(encrypted.to_vec())
}

fn weixin_oc_normalize_image_mime(raw: &[u8]) -> String {
    match image::guess_format(raw) {
        Ok(image::ImageFormat::Png) => "image/png".to_string(),
        Ok(image::ImageFormat::Jpeg) => "image/jpeg".to_string(),
        Ok(image::ImageFormat::Gif) => "image/gif".to_string(),
        Ok(image::ImageFormat::WebP) => "image/webp".to_string(),
        _ => "image/jpeg".to_string(),
    }
}

fn weixin_oc_guess_attachment_mime(file_name: &str, fallback: &str) -> String {
    media_mime_from_path(std::path::Path::new(file_name))
        .unwrap_or(fallback)
        .to_string()
}

fn weixin_oc_build_attachment_meta(
    state: &AppState,
    file_name: &str,
    mime: &str,
    raw: &[u8],
) -> Result<(AttachmentMetaInput, String), String> {
    let saved = persist_raw_attachment_to_downloads(state, file_name, mime, raw)?;
    let relative_path = workspace_relative_path(state, &saved);
    let final_file_name = saved
        .file_name()
        .and_then(|value| value.to_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(file_name)
        .to_string();
    Ok((
        AttachmentMetaInput {
            file_name: final_file_name,
            relative_path: relative_path.clone(),
            mime: mime.to_string(),
        },
        relative_path,
    ))
}

async fn weixin_oc_collect_media(
    state: &AppState,
    client: &reqwest::Client,
    item_list: &[WeixinOcMessageItem],
) -> Result<WeixinOcCollectedMedia, String> {
    let mut images = Vec::<BinaryPart>::new();
    let mut audios = Vec::<BinaryPart>::new();
    let mut attachments = Vec::<AttachmentMetaInput>::new();
    for item in item_list {
        let item_type = item.item_type.unwrap_or(0);
        let (media, file_name, fallback_mime, aes_key_override) = match item_type {
            2 => {
                let Some(image_item) = item.image_item.as_ref() else {
                    continue;
                };
                let Some(media) = image_item.media.as_ref() else {
                    continue;
                };
                (
                    media,
                    "image.jpg".to_string(),
                    "image/jpeg".to_string(),
                    image_item
                        .aeskey
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(|value| B64.encode(value)),
                )
            }
            3 => {
                let Some(voice_item) = item.voice_item.as_ref() else {
                    continue;
                };
                let Some(media) = voice_item.media.as_ref() else {
                    continue;
                };
                (
                    media,
                    "voice.silk".to_string(),
                    "audio/x-silk".to_string(),
                    None,
                )
            }
            4 => {
                let Some(file_item) = item.file_item.as_ref() else {
                    continue;
                };
                let Some(media) = file_item.media.as_ref() else {
                    continue;
                };
                let file_name = file_item
                    .file_name
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("file.bin")
                    .to_string();
                let mime = weixin_oc_guess_attachment_mime(&file_name, "application/octet-stream");
                (
                    media,
                    file_name.clone(),
                    mime,
                    None,
                )
            }
            5 => {
                let Some(video_item) = item.video_item.as_ref() else {
                    continue;
                };
                let Some(media) = video_item.media.as_ref() else {
                    continue;
                };
                (
                    media,
                    "video.mp4".to_string(),
                    "video/mp4".to_string(),
                    None,
                )
            }
            _ => continue,
        };
        let Some(encrypted_query_param) = media
            .encrypt_query_param
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let aes_key_value = aes_key_override.or_else(|| {
            media.aes_key
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        });
        let raw = weixin_oc_download_image_bytes(
            client,
            encrypted_query_param,
            aes_key_value.as_deref(),
        )
        .await?;
        let mime = if item_type == 2 {
            weixin_oc_normalize_image_mime(&raw)
        } else {
            fallback_mime
        };
        let (attachment, relative_path) =
            weixin_oc_build_attachment_meta(state, &file_name, &mime, &raw)?;
        let bytes_base64 = B64.encode(&raw);
        attachments.push(attachment);
        match item_type {
            2 => images.push(BinaryPart {
                mime,
                bytes_base64,
                saved_path: Some(relative_path),
            }),
            3 => audios.push(BinaryPart {
                mime,
                bytes_base64,
                saved_path: Some(relative_path),
            }),
            4 | 5 => {}
            _ => {}
        }
    }
    Ok(WeixinOcCollectedMedia {
        images,
        audios,
        attachments,
    })
}

