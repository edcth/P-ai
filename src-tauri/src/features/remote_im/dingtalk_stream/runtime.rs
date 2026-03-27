const DINGTALK_STREAM_TOPIC: &str = "/v1.0/im/bot/messages/get";
const DINGTALK_DOWNLOAD_API: &str = "https://api.dingtalk.com/v1.0/robot/messageFiles/download";
const DINGTALK_RECONNECT_INTERVAL_SECS: u64 = 30;
const DINGTALK_MAX_DOWNLOAD_SIZE_BYTES: u64 = 20 * 1024 * 1024;

#[derive(Debug, Clone)]
struct DingtalkRuntimeState {
    connected: bool,
    connected_at: Option<chrono::DateTime<chrono::Utc>>,
    endpoint: Option<String>,
    last_error: Option<String>,
}

impl Default for DingtalkRuntimeState {
    fn default() -> Self {
        Self {
            connected: false,
            connected_at: None,
            endpoint: None,
            last_error: None,
        }
    }
}

pub struct DingtalkStreamManager {
    states: std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<String, DingtalkRuntimeState>>>,
    stop_senders: std::sync::Arc<
        tokio::sync::RwLock<std::collections::HashMap<String, tokio::sync::watch::Sender<bool>>>,
    >,
    tasks: std::sync::Arc<
        tokio::sync::RwLock<std::collections::HashMap<String, tauri::async_runtime::JoinHandle<()>>>,
    >,
}

impl DingtalkStreamManager {
    pub fn new() -> Self {
        Self {
            states: std::sync::Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            stop_senders: std::sync::Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            tasks: std::sync::Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        }
    }

    async fn set_state(
        &self,
        channel_id: &str,
        connected: bool,
        endpoint: Option<String>,
        last_error: Option<String>,
    ) {
        let mut states = self.states.write().await;
        let state = states
            .entry(channel_id.to_string())
            .or_insert_with(DingtalkRuntimeState::default);
        let was_connected = state.connected;
        state.connected = connected;
        state.endpoint = endpoint;
        state.last_error = last_error;
        state.connected_at = if connected {
            if was_connected {
                state.connected_at
            } else {
                Some(chrono::Utc::now())
            }
        } else {
            None
        };
    }

    async fn add_log(&self, channel_id: &str, level: &str, message: &str) {
        let manager = onebot_v11_ws_manager();
        manager.add_log(channel_id, level, message).await;
    }

    pub(crate) async fn stop_channel(&self, channel_id: &str) {
        if let Some(tx) = self.stop_senders.write().await.remove(channel_id) {
            let _ = tx.send(true);
        }
        if let Some(handle) = self.tasks.write().await.remove(channel_id) {
            if let Err(err) = handle.await {
                self.add_log(
                    channel_id,
                    "warn",
                    &format!("[钉钉生命周期] task={} status=失败 trigger=stop_channel error={err}", channel_id),
                )
                .await;
            }
        }
        self.set_state(channel_id, false, None, None).await;
    }

    pub(crate) async fn reconcile_channel_runtime(&self, channel: &RemoteImChannelConfig, state: AppState) -> Result<(), String> {
        self.stop_channel(&channel.id).await;
        if channel.enabled && channel.platform == RemoteImPlatform::Dingtalk {
            self.start_channel(channel.clone(), state).await?;
        }
        Ok(())
    }

    pub(crate) async fn start_channel(&self, channel: RemoteImChannelConfig, state: AppState) -> Result<(), String> {
        let channel_id = channel.id.clone();
        self.stop_channel(&channel_id).await;
        let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
        self.stop_senders
            .write()
            .await
            .insert(channel_id.clone(), stop_tx);

        let task_channel_id = channel_id.clone();
        let manager = dingtalk_stream_manager();
        let handle = tauri::async_runtime::spawn(async move {
            manager
                .add_log(
                    &task_channel_id,
                    "info",
                    &format!(
                        "[钉钉生命周期] task={} status=开始 trigger=start_channel key_counts=0 duration_ms=0",
                        task_channel_id
                    ),
                )
                .await;
            let mut stop_rx = stop_rx;
            loop {
                if *stop_rx.borrow() {
                    break;
                }
                let result = tokio::select! {
                    changed = stop_rx.changed() => {
                        match changed {
                            Ok(()) => {
                                if *stop_rx.borrow() {
                                    break;
                                }
                                continue;
                            }
                            Err(_) => break,
                        }
                    }
                    ret = run_single_dingtalk_stream_session(&channel, &state) => ret,
                };
                match result {
                    Ok(()) => break,
                    Err(err) => {
                        manager
                            .set_state(&task_channel_id, false, None, Some(err.clone()))
                            .await;
                        manager
                            .add_log(
                                &task_channel_id,
                                "warn",
                                &format!(
                                    "[钉钉生命周期] task={} status=失败 trigger=run_session backoff_secs={} error={}",
                                    task_channel_id,
                                    DINGTALK_RECONNECT_INTERVAL_SECS,
                                    err
                                ),
                            )
                            .await;
                    }
                }
                tokio::select! {
                    changed = stop_rx.changed() => {
                        match changed {
                            Ok(()) => {
                                if *stop_rx.borrow() {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_secs(DINGTALK_RECONNECT_INTERVAL_SECS)) => {}
                }
            }
            manager
                .set_state(&task_channel_id, false, None, None)
                .await;
            manager
                .add_log(
                    &task_channel_id,
                    "info",
                    &format!(
                        "[钉钉生命周期] task={} status=完成 trigger=stop_channel key_counts=0 duration_ms=0",
                        task_channel_id
                    ),
                )
                .await;
        });
        self.tasks.write().await.insert(channel_id, handle);
        Ok(())
    }

    pub(crate) async fn get_channel_status(&self, channel_id: &str) -> ChannelConnectionStatus {
        let state = self
            .states
            .read()
            .await
            .get(channel_id)
            .cloned()
            .unwrap_or_default();
        ChannelConnectionStatus {
            channel_id: channel_id.to_string(),
            connected: state.connected,
            peer_addr: state.endpoint.clone(),
            connected_at: state.connected_at,
            listen_addr: String::new(),
            status_text: None,
            last_error: state.last_error.clone(),
            account_id: None,
            base_url: None,
            login_session_key: None,
            qrcode_url: None,
        }
    }
}

impl Default for DingtalkStreamManager {
    fn default() -> Self {
        Self::new()
    }
}

static DINGTALK_STREAM_MANAGER: once_cell::sync::Lazy<std::sync::Arc<DingtalkStreamManager>> =
    once_cell::sync::Lazy::new(|| std::sync::Arc::new(DingtalkStreamManager::new()));

pub fn dingtalk_stream_manager() -> std::sync::Arc<DingtalkStreamManager> {
    DINGTALK_STREAM_MANAGER.clone()
}
