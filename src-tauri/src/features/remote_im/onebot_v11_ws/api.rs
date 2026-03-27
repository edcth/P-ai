impl OnebotV11WsManager {
    /// 调用 OneBot API 并等待响应
    pub async fn call_api(
        &self,
        channel_id: &str,
        action: &str,
        params: Value,
        timeout_ms: u64,
    ) -> Result<Value, String> {
        let connections = self.connections.read().await;
        let conn = connections
            .get(channel_id)
            .ok_or_else(|| format!("渠道 {} 未连接", channel_id))?;
        let pending_responses = conn.pending_responses.clone();

        // 生成唯一 echo
        let echo = uuid::Uuid::new_v4().to_string();

        // 创建响应等待通道
        let (tx, rx) = oneshot::channel();
        pending_responses.write().await.insert(echo.clone(), tx);

        // 构建请求
        let request = OneBotApiRequest {
            action: action.to_string(),
            params,
            echo: Some(serde_json::json!(echo.clone())),
        };

        let payload = serde_json::to_string(&request)
            .map_err(|e| format!("序列化请求失败: {}", e))?;

        // 发送请求
        conn.tx
            .send(payload)
            .map_err(|e| format!("发送失败: {}", e))?;

        // 释放连接锁，等待响应
        drop(connections);

        // 等待响应或超时
        let result = tokio::time::timeout(
            Duration::from_millis(timeout_ms),
            rx
        ).await;

        match result {
            Ok(Ok(response)) => {
                if response.status == "ok" {
                    Ok(response.data)
                } else {
                    Err(format!("API 调用失败: status={}, retcode={}", response.status, response.retcode))
                }
            }
            Ok(Err(_)) => Err("响应通道已关闭".to_string()),
            Err(_) => {
                // 超时，清理 pending
                pending_responses.write().await.remove(&echo);
                Err(format!("API 调用超时 ({}ms)", timeout_ms))
            }
        }
    }
}
