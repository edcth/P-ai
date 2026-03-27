impl WeixinOcManager {
    async fn add_log(&self, channel_id: &str, level: &str, message: &str) {
        onebot_v11_ws_manager().add_log(channel_id, level, message).await;
    }

    async fn set_state<F>(&self, channel_id: &str, update: F)
    where
        F: FnOnce(&mut WeixinOcRuntimeState),
    {
        let mut states = self.states.write().await;
        let state = states
            .entry(channel_id.to_string())
            .or_insert_with(WeixinOcRuntimeState::default);
        let was_connected = state.connected;
        update(state);
        if state.connected && !was_connected {
            state.connected_at = Some(chrono::Utc::now());
        }
        if !state.connected {
            state.connected_at = None;
        }
    }

    async fn load_state_from_channel(&self, channel: &RemoteImChannelConfig) {
        let creds = WeixinOcCredentials::from_value(&channel.credentials);
        self.set_state(&channel.id, |state| {
            state.base_url = creds.normalized_base_url();
            state.account_id = creds.account_id.trim().to_string();
            state.user_id = creds.user_id.trim().to_string();
            if state.login_status == "idle" && !creds.token.trim().is_empty() {
                state.login_status = "logged_in".to_string();
            }
        })
        .await;
    }

    async fn build_status(&self, channel_id: &str) -> ChannelConnectionStatus {
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
            peer_addr: if state.account_id.trim().is_empty() {
                None
            } else {
                Some(state.account_id.clone())
            },
            connected_at: state.connected_at,
            listen_addr: String::new(),
            status_text: Some(state.login_status),
            last_error: if state.last_error.trim().is_empty() {
                None
            } else {
                Some(state.last_error)
            },
            account_id: if state.account_id.trim().is_empty() {
                None
            } else {
                Some(state.account_id)
            },
            base_url: Some(state.base_url),
            login_session_key: if state.session_key.trim().is_empty() {
                None
            } else {
                Some(state.session_key)
            },
            qrcode_url: if state.qrcode_img_content.trim().is_empty() {
                None
            } else {
                Some(state.qrcode_img_content)
            },
        }
    }

    async fn set_context_token(&self, channel_id: &str, user_id: &str, token: &str) {
        if user_id.trim().is_empty() || token.trim().is_empty() {
            return;
        }
        self.context_tokens.write().await.insert(
            format!("{}:{}", channel_id.trim(), user_id.trim()),
            token.trim().to_string(),
        );
    }

    async fn get_context_token(&self, channel_id: &str, user_id: &str) -> Option<String> {
        self.context_tokens
            .read()
            .await
            .get(&format!("{}:{}", channel_id.trim(), user_id.trim()))
            .cloned()
    }

    async fn stop_channel(&self, channel_id: &str) {
        if let Some(tx) = self.stop_senders.write().await.remove(channel_id) {
            let _ = tx.send(true);
        }
        if let Some(handle) = self.tasks.write().await.remove(channel_id) {
            let _ = handle.await;
        }
        self.set_state(channel_id, |state| {
            state.connected = false;
        })
        .await;
    }

    async fn reconcile_channel_runtime(
        &self,
        channel: &RemoteImChannelConfig,
        state: AppState,
    ) -> Result<(), String> {
        eprintln!(
            "[个人微信] reconcile_channel_runtime: channel_id={}, enabled={}, platform={:?}",
            channel.id, channel.enabled, channel.platform
        );
        self.load_state_from_channel(channel).await;
        self.stop_channel(&channel.id).await;
        if channel.platform != RemoteImPlatform::WeixinOc || !channel.enabled {
            eprintln!("[个人微信] 渠道已停用: channel_id={}", channel.id);
            self.add_log(&channel.id, "info", "[个人微信] 渠道已停用").await;
            return Ok(());
        }
        let creds = WeixinOcCredentials::from_value(&channel.credentials);
        eprintln!(
            "[个人微信] 当前凭证: channel_id={}, base_url={}, token_len={}, account_id={}, user_id={}",
            channel.id,
            creds.normalized_base_url(),
            creds.token.trim().len(),
            creds.account_id.trim(),
            creds.user_id.trim()
        );
        if creds.token.trim().is_empty() {
            eprintln!("[个人微信] 渠道已启用，但尚未登录（缺少 token）: channel_id={}", channel.id);
            self.set_state(&channel.id, |runtime| {
                runtime.connected = false;
                runtime.login_status = "need_login".to_string();
            })
            .await;
            self.add_log(&channel.id, "info", "[个人微信] 渠道已启用，但尚未登录（缺少 token）")
                .await;
            return Ok(());
        }
        eprintln!("[个人微信] 渠道已启用，正在启动轮询: channel_id={}", channel.id);
        self.add_log(&channel.id, "info", "[个人微信] 渠道已启用，正在启动轮询")
            .await;
        self.start_channel(channel.clone(), state).await
    }

    async fn start_channel(
        &self,
        channel: RemoteImChannelConfig,
        state: AppState,
    ) -> Result<(), String> {
        let channel_id = channel.id.clone();
        eprintln!("[个人微信] start_channel: channel_id={}", channel_id);
        let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
        self.stop_senders
            .write()
            .await
            .insert(channel_id.clone(), stop_tx);
        let manager = weixin_oc_manager();
        let task_channel_id = channel_id.clone();
        let handle = tauri::async_runtime::spawn(async move {
            manager
                .add_log(&task_channel_id, "info", "[个人微信] 轮询任务开始")
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
                    ret = run_single_weixin_oc_poll_cycle(&channel.id, &state) => ret,
                };
                if let Err(err) = result {
                    manager
                        .set_state(&task_channel_id, |runtime| {
                            runtime.connected = false;
                            runtime.last_error = err.clone();
                        })
                        .await;
                    manager
                        .add_log(
                            &task_channel_id,
                            "warn",
                            &format!("[个人微信] 拉取消息失败: {}", err),
                        )
                        .await;
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
                        _ = tokio::time::sleep(std::time::Duration::from_secs(3)) => {}
                    }
                } else {
                    manager
                        .set_state(&task_channel_id, |runtime| {
                            runtime.connected = true;
                            if runtime.login_status.trim().is_empty() || runtime.login_status == "need_login" {
                                runtime.login_status = "logged_in".to_string();
                            }
                        })
                        .await;
                }
            }
            manager
                .set_state(&task_channel_id, |runtime| {
                    runtime.connected = false;
                })
                .await;
            manager
                .add_log(&task_channel_id, "info", "[个人微信] 轮询任务结束")
                .await;
        });
        self.tasks.write().await.insert(channel_id, handle);
        Ok(())
    }
}

