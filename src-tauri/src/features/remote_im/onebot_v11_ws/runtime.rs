impl OnebotV11WsManager {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            channel_shutdowns: Arc::new(RwLock::new(HashMap::new())),
            channel_logs: Arc::new(RwLock::new(HashMap::new())),
            listen_addrs: Arc::new(RwLock::new(HashMap::new())),
            channel_tasks: Arc::new(RwLock::new(HashMap::new())),
            lifecycle_locks: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        }
    }

    async fn channel_lifecycle_guard(
        &self,
        channel_id: &str,
    ) -> tokio::sync::OwnedMutexGuard<()> {
        let lock = {
            let mut locks = self.lifecycle_locks.lock().await;
            locks
                .entry(channel_id.to_string())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone()
        };
        lock.lock_owned().await
    }

    /// 添加日志
    pub(crate) async fn add_log(&self, channel_id: &str, level: &str, message: &str) {
        let entry = ChannelLogEntry {
            timestamp: Utc::now(),
            level: level.to_string(),
            message: message.to_string(),
        };
        let mut logs = self.channel_logs.write().await;
        let channel_logs = logs.entry(channel_id.to_string()).or_insert_with(Vec::new);
        channel_logs.push(entry);
        // 只保留最近 100 条日志
        if channel_logs.len() > 100 {
            let start = channel_logs.len() - 100;
            channel_logs.drain(0..start);
        }
    }

    /// 停止单个渠道的 WebSocket 服务器
    pub async fn stop_channel(&self, channel_id: &str) -> Result<(), String> {
        let _guard = self.channel_lifecycle_guard(channel_id).await;
        self.stop_channel_inner(channel_id).await
    }

    async fn stop_channel_inner(&self, channel_id: &str) -> Result<(), String> {
        // 发送关闭信号给该渠道的 accept 循环
        if let Some(tx) = self.channel_shutdowns.write().await.remove(channel_id) {
            let _ = tx.send(());
        }
        // 等待旧的 accept 循环任务退出，确保端口被释放
        if let Some(handle) = self.channel_tasks.write().await.remove(channel_id) {
            let mut handle = handle;
            match tokio::time::timeout(Duration::from_secs(8), &mut handle).await {
                Ok(join_result) => {
                    if let Err(err) = join_result {
                        return Err(format!("停止渠道任务失败: {}", err));
                    }
                }
                Err(_) => {
                    handle.abort();
                    let _ = handle.await;
                    return Err(format!("停止渠道超时: {}", channel_id));
                }
            }
        }
        // 清除连接和监听地址
        self.connections.write().await.remove(channel_id);
        self.listen_addrs.write().await.remove(channel_id);
        self.add_log(channel_id, "info", "渠道服务器已停止").await;
        Ok(())
    }

    /// 依据渠道配置原子地收敛运行态：先停，再按 enabled/platform 决定是否启动
    pub(crate) async fn reconcile_channel_runtime(
        &self,
        channel: &RemoteImChannelConfig,
    ) -> Result<(), String> {
        let _guard = self.channel_lifecycle_guard(&channel.id).await;
        self.stop_channel_inner(&channel.id).await?;
        if channel.enabled && channel.platform == RemoteImPlatform::OnebotV11 {
            let credentials = OnebotV11WsCredentials::from_credentials(&channel.credentials);
            self.start_inner(channel.id.clone(), credentials).await?;
        }
        Ok(())
    }

    /// 获取渠道连接状态
    pub async fn get_connection_status(&self, channel_id: &str) -> ChannelConnectionStatus {
        let connections = self.connections.read().await;
        let listen_addrs = self.listen_addrs.read().await;

        if let Some(conn) = connections.get(channel_id) {
            ChannelConnectionStatus {
                channel_id: channel_id.to_string(),
                connected: true,
                peer_addr: conn.peer_addr.clone(),
                connected_at: conn.connected_at,
                listen_addr: listen_addrs.get(channel_id).cloned().unwrap_or_default(),
                status_text: None,
                last_error: None,
                account_id: None,
                base_url: None,
                login_session_key: None,
                qrcode_url: None,
            }
        } else {
            ChannelConnectionStatus {
                channel_id: channel_id.to_string(),
                connected: false,
                peer_addr: None,
                connected_at: None,
                listen_addr: listen_addrs.get(channel_id).cloned().unwrap_or_default(),
                status_text: None,
                last_error: None,
                account_id: None,
                base_url: None,
                login_session_key: None,
                qrcode_url: None,
            }
        }
    }

    /// 获取渠道日志
    pub async fn get_logs(&self, channel_id: &str) -> Vec<ChannelLogEntry> {
        let logs = self.channel_logs.read().await;
        logs.get(channel_id).cloned().unwrap_or_default()
    }

    /// 启动 WebSocket 服务器
    pub async fn start(
        &self,
        channel_id: String,
        credentials: OnebotV11WsCredentials,
    ) -> Result<(), String> {
        let _guard = self.channel_lifecycle_guard(&channel_id).await;
        self.start_inner(channel_id, credentials).await
    }

    async fn start_inner(
        &self,
        channel_id: String,
        credentials: OnebotV11WsCredentials,
    ) -> Result<(), String> {
        self.stop_channel_inner(&channel_id).await?;
        let addr: SocketAddr = format!("{}:{}", credentials.ws_host, credentials.ws_port)
            .parse()
            .map_err(|e| format!("无效地址: {}", e))?;

        let listener = TcpListener::bind(&addr)
            .await
            .map_err(|e| format!("绑定端口失败: {}", e))?;

        let listen_addr = addr.to_string();
        eprintln!("[远程IM][OneBot v11 WS] 渠道 {} 开始监听 {}", channel_id, listen_addr);

        // 记录监听地址
        self.listen_addrs.write().await.insert(channel_id.clone(), listen_addr.clone());
        self.add_log(&channel_id, "info", &format!("服务器启动，监听 {}", listen_addr)).await;

        let connections = self.connections.clone();
        let channel_logs = self.channel_logs.clone();

        // 创建 per-channel 的关闭信号
        let (shutdown_tx, _) = broadcast::channel::<()>(1);
        self.channel_shutdowns.write().await.insert(channel_id.clone(), shutdown_tx.clone());
        let mut shutdown_rx = shutdown_tx.subscribe();

        // 创建连接通道
        let (conn_tx, _) = broadcast::channel::<String>(64);
        let (event_tx, _) = broadcast::channel::<Value>(256);
        let pending_responses = Arc::new(RwLock::new(HashMap::<String, oneshot::Sender<OneBotApiResponse>>::new()));

        let saved_channel_id = channel_id.clone();
        let task_handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        eprintln!("[远程IM][OneBot v11 WS] 渠道 {} 收到关闭信号", channel_id);
                        let mut logs = channel_logs.write().await;
                        if let Some(l) = logs.get_mut(&channel_id) {
                            l.push(ChannelLogEntry {
                                timestamp: Utc::now(),
                                level: "info".to_string(),
                                message: "服务器关闭".to_string(),
                            });
                        }
                        break;
                    }
                    accept = listener.accept() => {
                        match accept {
                            Ok((stream, peer_addr)) => {
                                let channel_id = channel_id.clone();
                                let expected_token = credentials.ws_token.clone();
                                let conn_tx = conn_tx.clone();
                                let pending_responses = pending_responses.clone();
                                let event_tx = event_tx.clone();
                                let connections = connections.clone();
                                let channel_logs = channel_logs.clone();

                                tokio::spawn(async move {
                                    Self::handle_connection(
                                        stream,
                                        peer_addr,
                                        channel_id.clone(),
                                        expected_token,
                                        conn_tx,
                                        pending_responses,
                                        event_tx,
                                        connections,
                                        channel_logs,
                                    ).await;
                                });
                            }
                            Err(e) => {
                                eprintln!("[远程IM][OneBot v11 WS] 接受连接失败: {}", e);
                                let mut logs = channel_logs.write().await;
                                if let Some(l) = logs.get_mut(&channel_id) {
                                    l.push(ChannelLogEntry {
                                        timestamp: Utc::now(),
                                        level: "error".to_string(),
                                        message: format!("接受连接失败: {}", e),
                                    });
                                }
                            }
                        }
                    }
                }
            }
            // listener 在此处 drop，释放端口绑定
        });
        self.channel_tasks.write().await.insert(saved_channel_id, task_handle);

        Ok(())
    }

    async fn handle_connection(
        stream: tokio::net::TcpStream,
        peer_addr: SocketAddr,
        channel_id: String,
        expected_token: Option<String>,
        conn_tx: broadcast::Sender<String>,
        pending_responses: Arc<RwLock<HashMap<String, oneshot::Sender<OneBotApiResponse>>>>,
        event_tx: broadcast::Sender<Value>,
        connections: Arc<RwLock<HashMap<String, WsConnection>>>,
        channel_logs: Arc<RwLock<HashMap<String, Vec<ChannelLogEntry>>>>,
    ) {
        let expected_for_closure = expected_token.clone();
        let ws_result = accept_hdr_async(stream, move |req: &Request, response: Response| {
            if let Err(mut err_response) = validate_ws_token(req, expected_for_closure.as_deref()) {
                *err_response.headers_mut() = response.headers().clone();
                return Err(err_response);
            }
            Ok(response)
        })
        .await;

        let ws_stream = match ws_result {
            Ok(ws) => ws,
            Err(e) => {
                eprintln!("[远程IM][OneBot v11 WS] WebSocket 握手失败 {}: {}", peer_addr, e);
                append_channel_log(
                    &channel_logs,
                    &channel_id,
                    "error",
                    format!("WebSocket 握手失败 {}: {}", peer_addr, e),
                )
                .await;
                return;
            }
        };

        let peer_addr_str = peer_addr.to_string();
        eprintln!("[远程IM][OneBot v11 WS] 渠道 {} 客户端已连接: {}", channel_id, peer_addr_str);

        append_channel_log(
            &channel_logs,
            &channel_id,
            "info",
            format!("客户端已连接: {}", peer_addr_str),
        )
        .await;

        // 更新连接状态
        {
            let conn = WsConnection {
                tx: conn_tx.clone(),
                pending_responses: pending_responses.clone(),
                event_tx: event_tx.clone(),
                peer_addr: Some(peer_addr_str.clone()),
                connected_at: Some(Utc::now()),
            };
            connections.write().await.insert(channel_id.clone(), conn);
        }

        let (ws_sender, ws_receiver) = ws_stream.split();
        let cmd_rx = conn_tx.subscribe();
        run_message_loop(
            ws_sender,
            ws_receiver,
            cmd_rx,
            pending_responses,
            event_tx,
            connections,
            channel_logs,
            channel_id,
            peer_addr_str,
        )
        .await;
    }
}
