impl OnebotV11WsManager {
    /// 订阅事件流
    pub async fn subscribe_events(&self, channel_id: &str) -> Option<broadcast::Receiver<Value>> {
        let connections = self.connections.read().await;
        connections.get(channel_id).map(|c| c.event_tx.subscribe())
    }

    /// 关闭所有服务器
    pub async fn shutdown(&self) {
        let shutdowns = self.channel_shutdowns.write().await.drain().collect::<Vec<_>>();
        for (_, tx) in shutdowns {
            let _ = tx.send(());
        }
    }

    /// 检查渠道是否已连接
    pub async fn is_connected(&self, channel_id: &str) -> bool {
        self.connections.read().await.contains_key(channel_id)
    }

    /// 订阅渠道的关闭信号
    pub(crate) async fn subscribe_shutdown(&self, channel_id: &str) -> Option<broadcast::Receiver<()>> {
        let shutdowns = self.channel_shutdowns.read().await;
        shutdowns.get(channel_id).map(|tx| tx.subscribe())
    }
}

impl Default for OnebotV11WsManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 全局 OneBot v11 WebSocket 管理器
static ONEBOT_V11_WS_MANAGER: once_cell::sync::Lazy<Arc<OnebotV11WsManager>> =
    once_cell::sync::Lazy::new(|| Arc::new(OnebotV11WsManager::new()));

pub fn onebot_v11_ws_manager() -> Arc<OnebotV11WsManager> {
    ONEBOT_V11_WS_MANAGER.clone()
}

/// 启动 OneBot v11 WebSocket 服务器（同步版本，用于应用启动）
pub(crate) fn onebot_v11_ws_server_start(
    channel: RemoteImChannelConfig,
    _app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let credentials = OnebotV11WsCredentials::from_credentials(&channel.credentials);
    let manager = onebot_v11_ws_manager();
    let channel_id = channel.id.clone();

    // 通过 oneshot 等待异步启动结果，确保错误能传递给调用方
    let (tx, rx) = std::sync::mpsc::channel();
    tauri::async_runtime::spawn(async move {
        let result = manager.start(channel_id, credentials).await;
        if tx.send(result).is_err() {
            eprintln!("[远程IM][OneBot v11 WS] 启动结果回传失败：接收端已关闭");
        }
    });

    match rx.recv_timeout(Duration::from_secs(8)) {
        Ok(result) => result,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            Err("启动 OneBot v11 WS 服务超时，异步任务未及时返回".to_string())
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            Err("启动任务通道关闭".to_string())
        }
    }
}

/// 获取渠道连接状态
pub async fn get_channel_connection_status(channel_id: String) -> Result<ChannelConnectionStatus, String> {
    let manager = onebot_v11_ws_manager();
    Ok(manager.get_connection_status(&channel_id).await)
}

/// 获取渠道日志
pub async fn get_channel_logs(channel_id: String) -> Result<Vec<ChannelLogEntry>, String> {
    let manager = onebot_v11_ws_manager();
    Ok(manager.get_logs(&channel_id).await)
}

