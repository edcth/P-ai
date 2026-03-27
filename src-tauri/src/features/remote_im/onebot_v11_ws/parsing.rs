// ==================== OneBot v11 事件消费 ====================

#[derive(Debug, Clone)]
enum OnebotInboundMediaKind {
    Image,
    File,
}

#[derive(Debug, Clone)]
struct OnebotInboundMediaRef {
    kind: OnebotInboundMediaKind,
    file_ref: String,
    file_id: Option<String>,
    file_name: Option<String>,
    mime_hint: Option<String>,
}

#[derive(Debug, Clone)]
enum OnebotEmbeddedRefKind {
    Reply,
    Forward,
}

#[derive(Debug, Clone)]
struct OnebotEmbeddedRef {
    kind: OnebotEmbeddedRefKind,
    id: String,
}

fn onebot_embedded_ref_id(data: Option<&Value>) -> Option<String> {
    data.and_then(|d| d.get("id"))
        .and_then(|v| {
            v.as_str()
                .map(str::to_string)
                .or_else(|| v.as_u64().map(|n| n.to_string()))
                .or_else(|| v.as_i64().map(|n| n.to_string()))
        })
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// 从 OneBot v11 message 数组格式中提取文本、媒体引用和嵌入引用
fn parse_onebot_message_array(
    segments: &[Value],
) -> (String, Vec<OnebotInboundMediaRef>, Vec<OnebotEmbeddedRef>) {
    let mut texts = Vec::new();
    let mut media_refs = Vec::<OnebotInboundMediaRef>::new();
    let mut embedded_refs = Vec::<OnebotEmbeddedRef>::new();

    for seg in segments {
        let seg_type = seg.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let data = seg.get("data");
        match seg_type {
            "text" => {
                if let Some(text) = data.and_then(|d| d.get("text")).and_then(|v| v.as_str()) {
                    if !text.is_empty() {
                        texts.push(text.to_string());
                    }
                }
            }
            "image" => {
                let file_ref = data
                    .and_then(|d| d.get("url"))
                    .and_then(Value::as_str)
                    .or_else(|| data.and_then(|d| d.get("file")).and_then(Value::as_str))
                    .map(str::trim)
                    .unwrap_or("");
                if !file_ref.is_empty() {
                    media_refs.push(OnebotInboundMediaRef {
                        kind: OnebotInboundMediaKind::Image,
                        file_ref: file_ref.to_string(),
                        file_id: data
                            .and_then(|d| d.get("file_id"))
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|v| !v.is_empty())
                            .map(ToOwned::to_owned),
                        file_name: data
                            .and_then(|d| d.get("name"))
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|v| !v.is_empty())
                            .map(ToOwned::to_owned),
                        mime_hint: None,
                    });
                }
            }
            "at" => {
                let qq = data
                    .and_then(|d| d.get("qq"))
                    .and_then(|v| v.as_str().map(String::from).or_else(|| v.as_u64().map(|n| n.to_string())));
                if let Some(qq) = qq {
                    texts.push(format!("@{}", qq));
                }
            }
            "face" => {
                let face_id = data
                    .and_then(|d| d.get("id"))
                    .and_then(|v| v.as_str().map(String::from).or_else(|| v.as_u64().map(|n| n.to_string())));
                if let Some(id) = face_id {
                    texts.push(format!("[表情:{}]", id));
                }
            }
            "file" => {
                let file_ref = data
                    .and_then(|d| d.get("url"))
                    .and_then(Value::as_str)
                    .or_else(|| data.and_then(|d| d.get("file")).and_then(Value::as_str))
                    .map(str::trim)
                    .unwrap_or("");
                if !file_ref.is_empty() {
                    media_refs.push(OnebotInboundMediaRef {
                        kind: OnebotInboundMediaKind::File,
                        file_ref: file_ref.to_string(),
                        file_id: data
                            .and_then(|d| d.get("file_id"))
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|v| !v.is_empty())
                            .map(ToOwned::to_owned),
                        file_name: data
                            .and_then(|d| d.get("name"))
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|v| !v.is_empty())
                            .map(ToOwned::to_owned),
                        mime_hint: None,
                    });
                } else {
                    texts.push("[file]".to_string());
                }
            }
            "reply" => {
                if let Some(id) = onebot_embedded_ref_id(data) {
                    embedded_refs.push(OnebotEmbeddedRef {
                        kind: OnebotEmbeddedRefKind::Reply,
                        id,
                    });
                } else {
                    texts.push("[reply]".to_string());
                }
            }
            "forward" => {
                if let Some(id) = onebot_embedded_ref_id(data) {
                    embedded_refs.push(OnebotEmbeddedRef {
                        kind: OnebotEmbeddedRefKind::Forward,
                        id,
                    });
                } else {
                    texts.push("[forward]".to_string());
                }
            }
            "record" | "video" | "poke" => {
                texts.push(format!("[{}]", seg_type));
            }
            _ => {}
        }
    }
    (texts.join(""), media_refs, embedded_refs)
}

fn onebot_unescape_cq_value(raw: &str) -> String {
    raw.replace("&amp;", "&")
        .replace("&#91;", "[")
        .replace("&#93;", "]")
        .replace("&#44;", ",")
}

fn onebot_cq_param_value(params: &str, key: &str) -> Option<String> {
    let target = key.trim();
    if target.is_empty() {
        return None;
    }
    for pair in params.split(',') {
        let Some((raw_key, raw_value)) = pair.split_once('=') else {
            continue;
        };
        if raw_key.trim() != target {
            continue;
        }
        let value = onebot_unescape_cq_value(raw_value.trim());
        if value.is_empty() {
            continue;
        }
        return Some(value);
    }
    None
}

fn onebot_media_ref_from_cq(cq_type: &str, params: &str) -> Option<OnebotInboundMediaRef> {
    match cq_type.trim() {
        "image" => {
            let file_ref = onebot_cq_param_value(params, "url")
                .or_else(|| onebot_cq_param_value(params, "file"))?;
            Some(OnebotInboundMediaRef {
                kind: OnebotInboundMediaKind::Image,
                file_ref,
                file_id: onebot_cq_param_value(params, "file_id")
                    .or_else(|| onebot_cq_param_value(params, "id")),
                file_name: onebot_cq_param_value(params, "name"),
                mime_hint: None,
            })
        }
        "file" | "record" | "video" => {
            let file_ref = onebot_cq_param_value(params, "url")
                .or_else(|| onebot_cq_param_value(params, "file"))
                .unwrap_or_default();
            let file_id = onebot_cq_param_value(params, "file_id")
                .or_else(|| onebot_cq_param_value(params, "fid"))
                .or_else(|| onebot_cq_param_value(params, "id"));
            if file_ref.is_empty() && file_id.is_none() {
                return None;
            }
            Some(OnebotInboundMediaRef {
                kind: OnebotInboundMediaKind::File,
                file_ref,
                file_id,
                file_name: onebot_cq_param_value(params, "name"),
                mime_hint: match cq_type.trim() {
                    "record" => Some("audio/x-silk".to_string()),
                    "video" => Some("video/mp4".to_string()),
                    _ => None,
                },
            })
        }
        _ => None,
    }
}

/// 从 CQ 码字符串中提取文本、媒体引用与嵌入引用
fn parse_onebot_cq_string(
    raw: &str,
) -> (String, Vec<OnebotInboundMediaRef>, Vec<OnebotEmbeddedRef>) {
    let mut text = String::new();
    let mut media_refs = Vec::<OnebotInboundMediaRef>::new();
    let mut embedded_refs = Vec::<OnebotEmbeddedRef>::new();
    let mut cursor = 0usize;
    while cursor < raw.len() {
        let rest = &raw[cursor..];
        let Some(start_rel) = rest.find("[CQ:") else {
            text.push_str(&onebot_unescape_cq_value(rest));
            break;
        };
        let start = cursor + start_rel;
        if start > cursor {
            text.push_str(&onebot_unescape_cq_value(&raw[cursor..start]));
        }
        let after_start = &raw[(start + 4)..];
        let Some(end_rel) = after_start.find(']') else {
            text.push_str(&onebot_unescape_cq_value(&raw[start..]));
            break;
        };
        let cq_body = &after_start[..end_rel];
        cursor = start + 4 + end_rel + 1;

        let (cq_type, params) = cq_body
            .split_once(',')
            .map(|(left, right)| (left.trim(), right))
            .unwrap_or((cq_body.trim(), ""));
        if let Some(media_ref) = onebot_media_ref_from_cq(cq_type, params) {
            media_refs.push(media_ref);
            continue;
        }
        match cq_type {
            "at" => {
                if let Some(qq) = onebot_cq_param_value(params, "qq") {
                    text.push_str(&format!("@{}", qq));
                }
            }
            "face" => {
                if let Some(id) = onebot_cq_param_value(params, "id") {
                    text.push_str(&format!("[表情:{}]", id));
                }
            }
            "reply" => {
                if let Some(id) = onebot_cq_param_value(params, "id") {
                    embedded_refs.push(OnebotEmbeddedRef {
                        kind: OnebotEmbeddedRefKind::Reply,
                        id,
                    });
                } else {
                    text.push_str("[reply]");
                }
            }
            "forward" => {
                if let Some(id) = onebot_cq_param_value(params, "id") {
                    embedded_refs.push(OnebotEmbeddedRef {
                        kind: OnebotEmbeddedRefKind::Forward,
                        id,
                    });
                } else {
                    text.push_str("[forward]");
                }
            }
            "poke" => {
                text.push_str(&format!("[{}]", cq_type));
            }
            _ => {}
        }
    }
    (text, media_refs, embedded_refs)
}

fn extract_message_content(
    event: &Value,
) -> (
    String,
    Vec<OnebotInboundMediaRef>,
    Vec<OnebotEmbeddedRef>,
) {
    let message_field = event.get("message");
    if let Some(arr) = message_field.and_then(|v| v.as_array()) {
        let result = parse_onebot_message_array(arr);
        eprintln!(
            "[远程IM][OneBot v11 事件] 解析数组格式 message: text_len={}, media_items={}, embedded_refs={}",
            result.0.len(),
            result.1.len(),
            result.2.len()
        );
        return result;
    }
    if let Some(msg_str) = message_field.and_then(|v| v.as_str()) {
        let parsed = parse_onebot_cq_string(msg_str);
        eprintln!(
            "[远程IM][OneBot v11 事件] 解析字符串格式消息: text_len={}, media_items={}, embedded_refs={}",
            parsed.0.len(),
            parsed.1.len(),
            parsed.2.len()
        );
        return parsed;
    }
    if let Some(raw) = event.get("raw_message").and_then(|v| v.as_str()) {
        let parsed = parse_onebot_cq_string(raw);
        eprintln!(
            "[远程IM][OneBot v11 事件] 解析原始消息 raw_message: text_len={}, media_items={}, embedded_refs={}",
            parsed.0.len(),
            parsed.1.len(),
            parsed.2.len()
        );
        return parsed;
    }
    eprintln!(
        "[远程IM][OneBot v11 事件] message 字段类型未识别: {:?}",
        message_field.map(|v| v.to_string())
    );
    (String::new(), Vec::new(), Vec::new())
}

fn onebot_extract_nested_segments(value: &Value) -> Option<&[Value]> {
    value
        .get("message")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .or_else(|| value.get("messages").and_then(Value::as_array).map(Vec::as_slice))
        .or_else(|| value.get("nodes").and_then(Value::as_array).map(Vec::as_slice))
        .or_else(|| value.get("data").and_then(|v| v.get("message")).and_then(Value::as_array).map(Vec::as_slice))
        .or_else(|| value.get("data").and_then(|v| v.get("messages")).and_then(Value::as_array).map(Vec::as_slice))
}

fn onebot_parse_content_value(
    value: &Value,
) -> (
    String,
    Vec<OnebotInboundMediaRef>,
    Vec<OnebotEmbeddedRef>,
) {
    if let Some(segments) = value.as_array() {
        return parse_onebot_message_array(segments);
    }
    if let Some(text) = value.as_str() {
        return parse_onebot_cq_string(text);
    }
    if let Some(segments) = onebot_extract_nested_segments(value) {
        return parse_onebot_message_array(segments);
    }
    if let Some(content) = value.get("content") {
        return onebot_parse_content_value(content);
    }
    if let Some(content) = value.get("data").and_then(|v| v.get("content")) {
        return onebot_parse_content_value(content);
    }
    if let Some(text) = value
        .get("raw_message")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return parse_onebot_cq_string(text);
    }
    (String::new(), Vec::new(), Vec::new())
}

fn onebot_parse_forward_payload(
    value: &Value,
) -> (
    String,
    Vec<OnebotInboundMediaRef>,
) {
    let nodes = value
        .get("messages")
        .or_else(|| value.get("message"))
        .or_else(|| value.get("nodes"))
        .or_else(|| value.get("nodeList"))
        .or_else(|| value.get("data").and_then(|v| v.get("messages")))
        .or_else(|| value.get("data").and_then(|v| v.get("message")))
        .or_else(|| value.get("data").and_then(|v| v.get("nodes")))
        .or_else(|| value.get("data").and_then(|v| v.get("nodeList")));
    let Some(nodes) = nodes.and_then(Value::as_array) else {
        let (text, media_refs, _) = onebot_parse_content_value(value);
        return (text, media_refs);
    };

    let mut text_parts = Vec::<String>::new();
    let mut media_refs = Vec::<OnebotInboundMediaRef>::new();
    for node in nodes {
        let sender_name = onebot_resolve_forward_node_sender_name(node);
        let (text, media, _) = node
            .get("data")
            .and_then(|v| v.get("content"))
            .map(onebot_parse_content_value)
            .or_else(|| node.get("content").map(onebot_parse_content_value))
            .or_else(|| node.get("message").map(onebot_parse_content_value))
            .unwrap_or_else(|| onebot_parse_content_value(node));
        if !text.trim().is_empty() {
            text_parts.push(format!("{}：{}", sender_name, text.trim()));
        }
        media_refs.extend(media);
    }
    (text_parts.join("\n").trim().to_string(), media_refs)
}

fn onebot_read_sender_name(sender: &Value, prefer_card: bool) -> Option<String> {
    let primary_key = if prefer_card { "card" } else { "nickname" };
    let secondary_key = if prefer_card { "nickname" } else { "card" };

    sender
        .get(primary_key)
        .or_else(|| sender.get(secondary_key))
        .or_else(|| sender.get("user_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn onebot_resolve_forward_node_sender_name(node: &Value) -> String {
    node.get("sender")
        .and_then(|sender| onebot_read_sender_name(sender, false))
        .or_else(|| {
            node.get("data")
                .and_then(|data| data.get("sender"))
                .and_then(|sender| onebot_read_sender_name(sender, false))
        })
        .or_else(|| {
            node.get("data")
                .and_then(|data| data.get("name"))
                .or_else(|| node.get("name"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "未知发送者".to_string())
}

async fn onebot_call_action_try_params(
    manager: &OnebotV11WsManager,
    channel_id: &str,
    action: &str,
    params_list: &[Value],
) -> Result<Value, String> {
    let mut last_err = String::new();
    for params in params_list {
        match manager.call_api(channel_id, action, params.clone(), 5000).await {
            Ok(v) => return Ok(v),
            Err(err) => last_err = err,
        }
    }
    Err(format!(
        "all attempts failed for action={}, last_err={}",
        action, last_err
    ))
}

async fn onebot_expand_embedded_content(
    manager: &OnebotV11WsManager,
    channel_id: &str,
    refs: &[OnebotEmbeddedRef],
) -> (String, Vec<OnebotInboundMediaRef>) {
    let mut text_parts = Vec::<String>::new();
    let mut media_refs = Vec::<OnebotInboundMediaRef>::new();
    for item in refs {
        let payload_result = match item.kind {
            OnebotEmbeddedRefKind::Reply => {
                onebot_call_action_try_params(
                    manager,
                    channel_id,
                    "get_msg",
                    &[serde_json::json!({ "message_id": item.id })],
                )
                .await
            }
            OnebotEmbeddedRefKind::Forward => {
                onebot_call_action_try_params(
                    manager,
                    channel_id,
                    "get_forward_msg",
                    &[
                        serde_json::json!({ "id": item.id }),
                        serde_json::json!({ "message_id": item.id }),
                    ],
                )
                .await
            }
        };

        let Ok(payload) = payload_result else {
            match item.kind {
                OnebotEmbeddedRefKind::Reply => text_parts.push("[reply]".to_string()),
                OnebotEmbeddedRefKind::Forward => text_parts.push("[forward]".to_string()),
            }
            continue;
        };

        let (text, nested_media_refs) = match item.kind {
            OnebotEmbeddedRefKind::Reply => {
                let (text, media_refs, _) = onebot_parse_content_value(&payload);
                (text, media_refs)
            }
            OnebotEmbeddedRefKind::Forward => {
                let (text, media_refs) = onebot_parse_forward_payload(&payload);
                (text, media_refs)
            }
        };

        if !text.trim().is_empty() {
            let prefix = match item.kind {
                OnebotEmbeddedRefKind::Reply => "[引用]",
                OnebotEmbeddedRefKind::Forward => "[转发]",
            };
            text_parts.push(format!("{} {}", prefix, text.trim()));
        }
        media_refs.extend(nested_media_refs);
    }

    (text_parts.join("\n").trim().to_string(), media_refs)
}

async fn resolve_contact_info(
    event: &Value,
    manager: &OnebotV11WsManager,
    channel_id: &str,
) -> Result<(String, String, Option<String>), String> {
    let message_type = event
        .get("message_type")
        .and_then(|v| v.as_str())
        .unwrap_or("private");
    let user_id = event
        .get("user_id")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let group_id = event.get("group_id").and_then(|v| v.as_u64());
    if message_type == "group" {
        let gid = group_id.ok_or("群消息缺少 group_id")?;
        let group_name = match manager
            .call_api(channel_id, "get_group_info", serde_json::json!({"group_id": gid}), 5000)
            .await
        {
            Ok(info) => info
                .get("group_name")
                .and_then(|n| n.as_str())
                .map(String::from),
            Err(_) => None,
        };
        Ok(("group".to_string(), gid.to_string(), group_name))
    } else {
        Ok(("private".to_string(), user_id.to_string(), None))
    }
}

fn read_channel_config(
    state: &AppState,
    channel_id: &str,
) -> Result<Option<RemoteImChannelConfig>, String> {
    let config = state_read_config_cached(state)?;
    let channel_config = remote_im_channel_by_id(&config, channel_id).cloned();
    Ok(channel_config)
}

fn resolve_sender_name(event: &Value) -> String {
    event
        .get("sender")
        .and_then(|sender| onebot_read_sender_name(sender, true))
        .unwrap_or_else(|| "Unknown".to_string())
}

fn message_field_kind(message_field: Option<&Value>) -> &'static str {
    message_field
        .map(|v| match v {
            Value::Array(_) => "array",
            Value::String(_) => "string",
            Value::Null => "null",
            Value::Object(_) => "object",
            Value::Number(_) => "number",
            Value::Bool(_) => "bool",
        })
        .unwrap_or("missing")
}

