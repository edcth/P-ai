impl ConversationService {
    fn list_remote_im_contact_conversations(
        &self,
        state: &AppState,
    ) -> Result<Vec<RemoteImContactConversationSummary>, String> {
        let guard = state
            .conversation_lock
            .lock()
            .map_err(|err| state_lock_error_with_panic(file!(), line!(), module_path!(), &err))?;
        let runtime = state_read_runtime_state_cached(state)?;
        let mut unresolved_contact_ids = std::collections::HashSet::<String>::new();
        let mut resolved_pairs = Vec::<(RemoteImContact, String)>::new();
        for contact in runtime.remote_im_contacts.iter() {
            if let Some(conversation_id) = contact
                .bound_conversation_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                resolved_pairs.push((contact.clone(), conversation_id.to_string()));
            } else {
                unresolved_contact_ids.insert(contact.id.trim().to_string());
            }
        }
        if !unresolved_contact_ids.is_empty() {
            let conversation_key_map = runtime
                .remote_im_contacts
                .iter()
                .filter(|contact| unresolved_contact_ids.contains(contact.id.trim()))
                .map(|contact| {
                    (
                        remote_im_contact_conversation_key(contact),
                        contact.clone(),
                    )
                })
                .collect::<std::collections::HashMap<_, _>>();
            let chat_index = state_read_chat_index_cached(state)?;
            let fallback_pairs = chat_index
                .conversations
                .iter()
                .filter_map(|item| {
                    let conversation =
                        state_read_conversation_cached(state, item.id.as_str()).ok()?;
                    if conversation.summary.trim().is_empty()
                        || !conversation_is_remote_im_contact(&conversation)
                    {
                        return None;
                    }
                    let root_key = conversation
                        .root_conversation_id
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())?;
                    conversation_key_map
                        .get(root_key)
                        .cloned()
                        .map(|contact| (contact, conversation.id))
                })
                .collect::<Vec<_>>();
            resolved_pairs.extend(fallback_pairs);
        }
        let mut items = Vec::<RemoteImContactConversationSummary>::new();
        for (contact, conversation_id) in resolved_pairs {
            let store_paths = message_store::message_store_paths(&state.data_path, &conversation_id)?;
            let summary = if let Some(meta) = message_store::read_ready_message_store_meta(&store_paths)? {
                let manifest_status = message_store::read_message_store_manifest_status(&store_paths)?
                    .ok_or_else(|| format!("联系人会话缺少消息存储 manifest：{conversation_id}"))?;
                Some(RemoteImContactConversationSummary {
                    contact_id: contact.id.clone(),
                    conversation_id: conversation_id.clone(),
                    title: remote_im_contact_conversation_title(&contact),
                    updated_at: meta.updated_at().to_string(),
                    last_message_at: meta
                        .last_assistant_at()
                        .map(ToOwned::to_owned)
                        .or_else(|| meta.last_user_at().map(ToOwned::to_owned))
                        .or_else(|| Some(meta.updated_at().to_string())),
                    message_count: manifest_status.source_message_count,
                    channel_id: contact.channel_id.clone(),
                    platform: contact.platform.clone(),
                    contact_display_name: remote_im_contact_display_name(&contact),
                    bound_department_id: contact.bound_department_id.clone(),
                    processing_mode: normalize_contact_processing_mode(&contact.processing_mode),
                })
            } else {
                let conversation = match self.try_read_unarchived_conversation(state, &conversation_id)? {
                    Some(conversation) if conversation_is_remote_im_contact(&conversation) => conversation,
                    _ => continue,
                };
                Some(RemoteImContactConversationSummary {
                    contact_id: contact.id.clone(),
                    conversation_id: conversation.id.clone(),
                    title: remote_im_contact_conversation_title(&contact),
                    updated_at: conversation.updated_at.clone(),
                    last_message_at: conversation
                        .messages
                        .last()
                        .map(|message| message.created_at.clone()),
                    message_count: conversation.messages.len(),
                    channel_id: contact.channel_id.clone(),
                    platform: contact.platform.clone(),
                    contact_display_name: remote_im_contact_display_name(&contact),
                    bound_department_id: contact.bound_department_id.clone(),
                    processing_mode: normalize_contact_processing_mode(&contact.processing_mode),
                })
            };
            if let Some(item) = summary {
                items.push(item);
            }
        }
        items.sort_by(|a, b| {
            let bk = b
                .last_message_at
                .as_deref()
                .unwrap_or(b.updated_at.as_str());
            let ak = a
                .last_message_at
                .as_deref()
                .unwrap_or(a.updated_at.as_str());
            bk.cmp(ak).then_with(|| b.updated_at.cmp(&a.updated_at))
        });
        drop(guard);
        Ok(items)
    }

    fn read_remote_im_contact_conversation_messages(
        &self,
        state: &AppState,
        contact_id: &str,
    ) -> Result<Vec<ChatMessage>, String> {
        let normalized_contact_id = contact_id.trim();
        if normalized_contact_id.is_empty() {
            return Err("contact_id 为必填项。".to_string());
        }
        let guard = state
            .conversation_lock
            .lock()
            .map_err(|err| state_lock_error_with_panic(file!(), line!(), module_path!(), &err))?;
        let runtime = state_read_runtime_state_cached(state)?;
        let runtime_contact = runtime
            .remote_im_contacts
            .iter()
            .find(|item| item.id == normalized_contact_id)
            .cloned()
            .ok_or_else(|| format!("未找到远程联系人：{normalized_contact_id}"))?;
        let conversation_id = if let Some(conversation_id) = runtime_contact
            .bound_conversation_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            conversation_id.to_string()
        } else {
            let target_key = remote_im_contact_conversation_key(&runtime_contact);
            let chat_index = state_read_chat_index_cached(state)?;
            chat_index
                .conversations
                .iter()
                .filter_map(|item| state_read_conversation_cached(state, item.id.as_str()).ok())
                .find(|conversation| {
                    conversation.summary.trim().is_empty()
                        && conversation_is_remote_im_contact(conversation)
                        && conversation.root_conversation_id.as_deref().map(str::trim)
                            == Some(target_key.as_str())
                })
                .map(|conversation| conversation.id)
                .ok_or_else(|| format!("联系人未绑定联系人会话：{normalized_contact_id}"))?
        };
        drop(guard);
        let store_paths = message_store::message_store_paths(&state.data_path, &conversation_id)?;
        let mut messages = if let Some(page) =
            message_store::read_ready_message_store_recent_messages_page_cached(
                &store_paths,
                DEFAULT_FOREGROUND_SNAPSHOT_RECENT_LIMIT,
            )?
        {
                let _ = self.retain_message_store_block_cache_whitelist(state);
                page.messages
            } else {
                self.with_unarchived_conversation_by_id_fast(
                    state,
                    &conversation_id,
                    |conversation| {
                        let total = conversation.messages.len();
                        let start = total.saturating_sub(DEFAULT_FOREGROUND_SNAPSHOT_RECENT_LIMIT);
                        Ok(conversation.messages[start..].to_vec())
                    },
                )?
            };
        materialize_chat_message_parts_from_media_refs(&mut messages, &state.data_path);
        Ok(messages)
    }

    fn read_remote_im_contact_conversation_block_page(
        &self,
        state: &AppState,
        contact_id: &str,
        requested_block_id: Option<u32>,
    ) -> Result<ConversationBlockPageResult, String> {
        let normalized_contact_id = contact_id.trim();
        if normalized_contact_id.is_empty() {
            return Err("contact_id 为必填项。".to_string());
        }
        let guard = state
            .conversation_lock
            .lock()
            .map_err(|err| state_lock_error_with_panic(file!(), line!(), module_path!(), &err))?;
        let runtime = state_read_runtime_state_cached(state)?;
        let runtime_contact = runtime
            .remote_im_contacts
            .iter()
            .find(|item| item.id == normalized_contact_id)
            .cloned()
            .ok_or_else(|| format!("未找到远程联系人：{normalized_contact_id}"))?;
        let conversation_id = if let Some(conversation_id) = runtime_contact
            .bound_conversation_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            conversation_id.to_string()
        } else {
            let target_key = remote_im_contact_conversation_key(&runtime_contact);
            let chat_index = state_read_chat_index_cached(state)?;
            chat_index
                .conversations
                .iter()
                .filter_map(|item| state_read_conversation_cached(state, item.id.as_str()).ok())
                .find(|conversation| {
                    conversation.summary.trim().is_empty()
                        && conversation_is_remote_im_contact(conversation)
                        && conversation.root_conversation_id.as_deref().map(str::trim)
                            == Some(target_key.as_str())
                })
                .map(|conversation| conversation.id)
                .ok_or_else(|| format!("联系人未绑定联系人会话：{normalized_contact_id}"))?
        };
        let conversation = state_read_conversation_cached(state, &conversation_id)?;
        if !conversation.summary.trim().is_empty() || !conversation_is_remote_im_contact(&conversation) {
            drop(guard);
            return Err(format!("联系人未绑定联系人会话：{normalized_contact_id}"));
        }
        drop(guard);

        let store_paths = message_store::message_store_paths(&state.data_path, &conversation_id)?;
        if let Some(page) =
            message_store::read_ready_message_store_block_page(&store_paths, requested_block_id)?
        {
            let _ = self.retain_message_store_block_cache_whitelist(state);
            let mut messages = page.messages;
            materialize_chat_message_parts_from_media_refs(&mut messages, &state.data_path);
            return Ok(ConversationBlockPageResult {
                blocks: page
                    .blocks
                    .into_iter()
                    .map(|item| ConversationBlockSummaryResult {
                        block_id: item.block_id,
                        message_count: item.message_count,
                        first_message_id: item.first_message_id,
                        last_message_id: item.last_message_id,
                        first_created_at: item.first_created_at,
                        last_created_at: item.last_created_at,
                        is_latest: item.is_latest,
                    })
                    .collect(),
                selected_block_id: page.selected_block_id,
                messages,
                has_prev_block: page.has_prev_block,
                has_next_block: page.has_next_block,
            });
        }

        let mut messages = conversation.messages.clone();
        materialize_chat_message_parts_from_media_refs(&mut messages, &state.data_path);
        Ok(ConversationBlockPageResult {
            blocks: vec![ConversationBlockSummaryResult {
                block_id: 0,
                message_count: messages.len(),
                first_message_id: messages
                    .first()
                    .map(|message| message.id.clone())
                    .unwrap_or_default(),
                last_message_id: messages
                    .last()
                    .map(|message| message.id.clone())
                    .unwrap_or_default(),
                first_created_at: messages.first().map(|message| message.created_at.clone()),
                last_created_at: messages.last().map(|message| message.created_at.clone()),
                is_latest: true,
            }],
            selected_block_id: 0,
            messages,
            has_prev_block: false,
            has_next_block: false,
        })
    }

    fn clear_remote_im_contact_conversation(
        &self,
        state: &AppState,
        contact_id: &str,
    ) -> Result<bool, String> {
        let normalized_contact_id = contact_id.trim();
        if normalized_contact_id.is_empty() {
            return Err("contact_id 为必填项。".to_string());
        }
        let guard = state
            .conversation_lock
            .lock()
            .map_err(|err| state_lock_error_with_panic(file!(), line!(), module_path!(), &err))?;
        let runtime = state_read_runtime_state_cached(state)?;
        let Some(contact) = runtime
            .remote_im_contacts
            .iter()
            .find(|item| item.id == normalized_contact_id)
            .cloned()
        else {
            drop(guard);
            return Err(format!("未找到远程联系人：{normalized_contact_id}"));
        };
        let conversation_id = contact
            .bound_conversation_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| {
                let target_key = remote_im_contact_conversation_key(&contact);
                match state_read_chat_index_cached(state) {
                    Ok(chat_index) => chat_index
                        .conversations
                        .iter()
                        .filter_map(|item| match state_read_conversation_cached(state, item.id.as_str()) {
                            Ok(conversation) => Some(conversation),
                            Err(err) => {
                                runtime_log_warn(format!(
                                    "[联系人会话] 警告，任务=clear_remote_im_contact_conversation_lookup，conversation_id={}，contact_id={}，error={}",
                                    item.id,
                                    normalized_contact_id,
                                    err
                                ));
                                None
                            }
                        })
                        .find(|conversation| {
                            conversation.summary.trim().is_empty()
                                && conversation_is_remote_im_contact(conversation)
                                && conversation.root_conversation_id.as_deref().map(str::trim)
                                    == Some(target_key.as_str())
                        })
                        .map(|conversation| conversation.id),
                    Err(err) => {
                        runtime_log_warn(format!(
                            "[联系人会话] 警告，任务=clear_remote_im_contact_read_chat_index，contact_id={}，error={}",
                            normalized_contact_id, err
                        ));
                        None
                    }
                }
            });
        let Some(conversation_id) = conversation_id else {
            drop(guard);
            return Ok(false);
        };
        let mut conversation = match state_read_conversation_cached(state, &conversation_id) {
            Ok(conversation)
                if conversation.summary.trim().is_empty()
                    && conversation_is_remote_im_contact(&conversation) =>
            {
                conversation
            }
            _ => {
                drop(guard);
                return Ok(false);
            }
        };

        conversation.messages.clear();
        conversation.memory_recall_table.clear();
        conversation.last_user_at = None;
        conversation.last_assistant_at = None;
        conversation.status = "inactive".to_string();
        conversation.updated_at = now_iso();

        if let Err(err) = state_schedule_conversation_persist(state, &conversation, false) {
            drop(guard);
            return Err(err);
        }
        drop(guard);
        Ok(true)
    }
}
