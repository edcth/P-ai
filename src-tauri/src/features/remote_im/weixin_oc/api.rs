fn weixin_oc_random_wechat_uin() -> String {
    let bytes = *Uuid::new_v4().as_bytes();
    let value = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    B64.encode(value.to_string())
}

fn weixin_oc_is_login_confirmed(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        "confirmed" | "confirm" | "success" | "logged_in" | "login_success"
    )
}

fn weixin_oc_request_headers(
    body: &str,
    token: Option<&str>,
) -> Result<reqwest::header::HeaderMap, String> {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        reqwest::header::HeaderValue::from_static("application/json"),
    );
    headers.insert(
        "AuthorizationType",
        reqwest::header::HeaderValue::from_static("ilink_bot_token"),
    );
    headers.insert(
        "X-WECHAT-UIN",
        reqwest::header::HeaderValue::from_str(weixin_oc_random_wechat_uin().as_str())
            .map_err(|err| format!("构造 X-WECHAT-UIN 失败: {err}"))?,
    );
    headers.insert(
        reqwest::header::CONTENT_LENGTH,
        reqwest::header::HeaderValue::from_str(body.len().to_string().as_str())
            .map_err(|err| format!("构造 Content-Length 失败: {err}"))?,
    );
    if let Some(value) = token.map(str::trim).filter(|value| !value.is_empty()) {
        headers.insert(
            reqwest::header::AUTHORIZATION,
            reqwest::header::HeaderValue::from_str(format!("Bearer {value}").as_str())
                .map_err(|err| format!("构造 Authorization 失败: {err}"))?,
        );
    }
    Ok(headers)
}
