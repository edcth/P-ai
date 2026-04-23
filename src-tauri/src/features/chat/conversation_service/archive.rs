impl ConversationService {
    fn list_archives(&self, state: &AppState) -> Result<Vec<ArchiveSummary>, String> {
        let guard = state.conversation_lock.lock().map_err(|err| {
            named_lock_error("conversation_lock", file!(), line!(), module_path!(), &err)
        })?;

        let app_config = read_config(&state.config_path)?;
        let mut summaries = with_app_data_cached_ref(state, |data, _detail| {
            Ok(data
                .conversations
                .iter()
                .filter(|conversation| !conversation.summary.trim().is_empty())
                .map(|archive| {
                    let api_config_id = department_for_agent_id(&app_config, &archive.agent_id)
                        .map(department_primary_api_config_id)
                        .unwrap_or_default();
                    ArchiveSummary {
                        archive_id: archive.id.clone(),
                        archived_at: archive
                            .archived_at
                            .clone()
                            .unwrap_or_else(|| archive.updated_at.clone()),
                        title: if archive.title.trim().is_empty() {
                            archive_first_user_preview(archive, &app_config.ui_language)
                        } else {
                            archive.title.trim().to_string()
                        },
                        message_count: archive.messages.len(),
                        api_config_id,
                        agent_id: archive.agent_id.clone(),
                    }
                })
                .collect::<Vec<_>>())
        })?;
        summaries.sort_by(|a, b| b.archived_at.cmp(&a.archived_at));
        drop(guard);
        Ok(summaries)
    }

    fn read_archive_messages(
        &self,
        state: &AppState,
        archive_id: &str,
    ) -> Result<Vec<ChatMessage>, String> {
        let normalized_archive_id = archive_id.trim();
        if normalized_archive_id.is_empty() {
            return Err("archiveId is required".to_string());
        }
        let guard = state.conversation_lock.lock().map_err(|err| {
            named_lock_error("conversation_lock", file!(), line!(), module_path!(), &err)
        })?;
        let mut messages = with_app_data_cached_ref(state, |data, _detail| {
            data.conversations
                .iter()
                .find(|conversation| {
                    conversation.id == normalized_archive_id
                        && !conversation.summary.trim().is_empty()
                })
                .map(|conversation| conversation.messages.clone())
                .ok_or_else(|| "Archive not found".to_string())
        })?;
        drop(guard);
        materialize_chat_message_parts_from_media_refs(&mut messages, &state.data_path);
        Ok(messages)
    }

    fn read_archive_summary(&self, state: &AppState, archive_id: &str) -> Result<String, String> {
        let normalized_archive_id = archive_id.trim();
        if normalized_archive_id.is_empty() {
            return Err("archiveId is required".to_string());
        }
        let guard = state.conversation_lock.lock().map_err(|err| {
            named_lock_error("conversation_lock", file!(), line!(), module_path!(), &err)
        })?;
        let summary = with_app_data_cached_ref(state, |data, _detail| {
            data.conversations
                .iter()
                .find(|conversation| {
                    conversation.id == normalized_archive_id
                        && !conversation.summary.trim().is_empty()
                })
                .map(|conversation| conversation.summary.clone())
                .ok_or_else(|| "Archive not found".to_string())
        })?;
        drop(guard);
        Ok(summary)
    }

    fn resolve_archive_target_conversation(
        &self,
        state: &AppState,
        input: &SessionSelector,
    ) -> Result<(ApiConfig, ResolvedApiConfig, Conversation, String), String> {
        let guard = state.conversation_lock.lock().map_err(|err| {
            format!(
                "Failed to lock state mutex at {}:{} {}: {err}",
                file!(),
                line!(),
                module_path!()
            )
        })?;
        let mut app_config = read_config(&state.config_path)?;
        let mut data = state_read_app_data_cached(state)?;
        merge_private_organization_into_runtime_data(&state.data_path, &mut app_config, &mut data)?;
        let selected_api = resolve_selected_api_config(&app_config, input.api_config_id.as_deref())
            .ok_or_else(|| "No API config configured. Please add one.".to_string())?;
        let resolved_api = resolve_api_config(&app_config, Some(selected_api.id.as_str()))?;
        let requested_agent_id = input.agent_id.trim();
        let effective_agent_id =
            if data
                .agents
                .iter()
                .any(|agent| agent.id == requested_agent_id && !agent.is_built_in_user)
            {
                requested_agent_id.to_string()
            } else if data.agents.iter().any(|agent| {
                agent.id == data.assistant_department_agent_id && !agent.is_built_in_user
            }) {
                data.assistant_department_agent_id.clone()
            } else {
                data.agents
                    .iter()
                    .find(|agent| !agent.is_built_in_user)
                    .map(|agent| agent.id.clone())
                    .ok_or_else(|| "Selected agent not found.".to_string())?
            };
        let requested_conversation_id = input
            .conversation_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let source_idx = if let Some(conversation_id) = requested_conversation_id {
            data.conversations.iter().position(|conversation| {
                conversation.id == conversation_id
                    && conversation.summary.trim().is_empty()
                    && !conversation_is_delegate(conversation)
            })
        } else {
            latest_active_conversation_index(&data, &selected_api.id, &effective_agent_id)
        }
        .ok_or_else(|| "当前没有可归档的活动对话。".to_string())?;
        let source = data
            .conversations
            .get(source_idx)
            .cloned()
            .ok_or_else(|| "当前没有可归档的活动对话。".to_string())?;
        drop(guard);
        Ok((selected_api, resolved_api, source, effective_agent_id))
    }

    fn delete_archive(&self, state: &AppState, archive_id: &str) -> Result<(), String> {
        let normalized_archive_id = archive_id.trim();
        if normalized_archive_id.is_empty() {
            return Err("archiveId is required".to_string());
        }
        let guard = state.conversation_lock.lock().map_err(|err| {
            named_lock_error("conversation_lock", file!(), line!(), module_path!(), &err)
        })?;

        let mut data = state_read_app_data_cached(state)?;
        let before = data.conversations.len();
        data.conversations.retain(|conversation| {
            !(conversation.id == normalized_archive_id && !conversation.summary.trim().is_empty())
        });
        if data.conversations.len() == before {
            drop(guard);
            return Err("Archive not found".to_string());
        }
        persist_removed_and_selected_conversations_and_runtime(
            state,
            &data,
            &[normalized_archive_id.to_string()],
            &[],
            "delete_archive",
        )?;
        drop(guard);
        Ok(())
    }

    fn prepare_background_archive_active_conversation(
        &self,
        state: &AppState,
        selected_api: &ApiConfig,
        source: &Conversation,
    ) -> Result<String, String> {
        let guard = state.conversation_lock.lock().map_err(|err| {
            format!(
                "Failed to lock state mutex at {}:{} {}: {err}",
                file!(),
                line!(),
                module_path!()
            )
        })?;
        let mut data = state_read_app_data_cached(state)?;
        let _ = normalize_single_active_main_conversation(&mut data);

        ensure_archive_source_available(&data, &source.id)?;
        let active_conversation_id = if let Some(conversation_id) =
            find_archive_restore_target_conversation_id(&data, &source.id)
        {
            conversation_id
        } else {
            let conversation =
                build_archive_replacement_conversation(state, &data, selected_api, source)?;
            let conversation_id = conversation.id.clone();
            data.conversations.push(conversation);
            if data
                .main_conversation_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_none()
            {
                data.main_conversation_id = Some(conversation_id.clone());
            }
            conversation_id
        };

        let app_config = state_read_config_cached(state)?;
        let overview_payload =
            build_unarchived_conversation_overview_payload(state, &app_config, &data);
        persist_selected_conversations_and_runtime(
            state,
            &data,
            std::slice::from_ref(&active_conversation_id),
            "restore_active_conversation_from_archive",
        )?;
        drop(guard);
        emit_unarchived_conversation_overview_updated_payload(state, &overview_payload);
        Ok(active_conversation_id)
    }

    fn import_archives(
        &self,
        state: &AppState,
        incoming_archives: &mut Vec<ConversationArchive>,
    ) -> Result<ImportArchivesMutationResult, String> {
        let guard = state.conversation_lock.lock().map_err(|err| {
            format!(
                "Failed to lock state mutex at {}:{} {}: {err}",
                file!(),
                line!(),
                module_path!()
            )
        })?;
        let mut data = state_read_app_data_cached(state)?;
        let data_before = data.clone();

        let mut index_by_conversation_id = std::collections::HashMap::<String, usize>::new();
        for (idx, conv) in data.conversations.iter().enumerate() {
            index_by_conversation_id.insert(conv.id.clone(), idx);
        }

        let mut imported_count = 0usize;
        let mut replaced_count = 0usize;
        let mut skipped_count = 0usize;
        let mut selected_archive_id: Option<String> = None;
        let mut seen_conversation_ids = std::collections::HashSet::<String>::new();

        for archive in incoming_archives.iter_mut() {
            normalize_archive_for_import(archive, &state.data_path);
        }

        for archive in incoming_archives.drain(..) {
            let archive_id = archive.archive_id.clone();
            let conversation = archive_to_conversation(archive);
            let conversation_id = conversation.id.clone();
            if !seen_conversation_ids.insert(conversation_id.clone()) {
                skipped_count += 1;
                continue;
            }
            if let Some(idx) = index_by_conversation_id.get(&conversation_id).copied() {
                data.conversations[idx] = conversation;
                replaced_count += 1;
            } else {
                data.conversations.push(conversation);
                index_by_conversation_id.insert(conversation_id, data.conversations.len() - 1);
                imported_count += 1;
            }
            if selected_archive_id.is_none() {
                selected_archive_id = Some(archive_id);
            }
        }

        persist_app_data_conversation_runtime_delta(state, &data_before, &data)?;
        let total_count = archived_conversations_from_data(&data).len();
        drop(guard);

        Ok(ImportArchivesMutationResult {
            imported_count,
            replaced_count,
            skipped_count,
            total_count,
            selected_archive_id,
        })
    }

    fn persist_compaction_message(
        &self,
        state: &AppState,
        source: &Conversation,
        compression_message: &ChatMessage,
        user_profile_snapshot: Option<String>,
    ) -> Result<CompactionMessagePersistResult, String> {
        let guard = state.conversation_lock.lock().map_err(|err| {
            format!(
                "Failed to lock state mutex at {}:{} {}: {err}",
                file!(),
                line!(),
                module_path!()
            )
        })?;
        let mut data = state_read_app_data_cached(state)?;

        let conversation_idx = data
            .conversations
            .iter()
            .position(|item| item.id == source.id && item.summary.trim().is_empty())
            .ok_or_else(|| "活动对话已变化，请重试上下文整理。".to_string())?;
        let compression_message_id = compression_message.id.clone();
        {
            let conversation = data
                .conversations
                .get_mut(conversation_idx)
                .ok_or_else(|| "活动对话索引无效，请重试上下文整理。".to_string())?;
            conversation.messages.push(compression_message.clone());
            conversation.user_profile_snapshot = user_profile_snapshot.unwrap_or_default();
            let now = now_iso();
            conversation.updated_at = now.clone();
            conversation.last_user_at = Some(now);
        }
        let active_conversation_id = data
            .conversations
            .get(conversation_idx)
            .map(|item| item.id.clone());
        persist_single_conversation_runtime_fast(state, &data, &source.id)?;

        drop(guard);

        {
            let verify_guard = state.conversation_lock.lock().map_err(|err| {
                format!(
                    "Failed to lock state mutex at {}:{} {}: {err}",
                    file!(),
                    line!(),
                    module_path!()
                )
            })?;
            let verify_data = state_read_app_data_cached(state)?;
            let persisted = verify_data
                .conversations
                .iter()
                .find(|item| item.id == source.id && item.summary.trim().is_empty())
                .map(|conversation| {
                    conversation
                        .messages
                        .iter()
                        .any(|message| message.id == compression_message_id)
                })
                .unwrap_or(false);
            drop(verify_guard);
            if !persisted {
                return Err(
                    "上下文整理消息写入校验失败：已执行整理但未找到落盘消息，请重试。".to_string(),
                );
            }
        }

        Ok(CompactionMessagePersistResult {
            active_conversation_id,
            compression_message_id,
        })
    }
}

fn ensure_archive_source_available(data: &AppData, source_id: &str) -> Result<(), String> {
    let found = data.conversations.iter().any(|conversation| {
        conversation.id == source_id
            && conversation.summary.trim().is_empty()
            && conversation_visible_in_foreground_lists(conversation)
    });
    if found {
        return Ok(());
    }
    Err("当前没有可归档的活动对话。".to_string())
}

fn find_archive_restore_target_conversation_id(data: &AppData, source_id: &str) -> Option<String> {
    data.conversations
        .iter()
        .enumerate()
        .filter(|(_, conversation)| {
            conversation.id != source_id
                && conversation.summary.trim().is_empty()
                && conversation_visible_in_foreground_lists(conversation)
        })
        .max_by(|(idx_a, a), (idx_b, b)| {
            let a_updated = a.updated_at.trim();
            let b_updated = b.updated_at.trim();
            let a_created = a.created_at.trim();
            let b_created = b.created_at.trim();
            a_updated
                .cmp(b_updated)
                .then_with(|| a_created.cmp(b_created))
                .then_with(|| idx_a.cmp(idx_b))
        })
        .map(|(_, conversation)| conversation.id.clone())
}

fn build_archive_replacement_conversation(
    state: &AppState,
    data: &AppData,
    selected_api: &ApiConfig,
    source: &Conversation,
) -> Result<Conversation, String> {
    let mut conversation = build_conversation_record(
        &selected_api.id,
        "",
        ASSISTANT_DEPARTMENT_ID,
        "",
        CONVERSATION_KIND_CHAT,
        None,
        None,
    );
    let profile_snapshot = data
        .agents
        .iter()
        .find(|item| item.id == data.assistant_department_agent_id)
        .and_then(|agent| match build_user_profile_snapshot_block(&state.data_path, agent, 12) {
            Ok(snapshot) => snapshot,
            Err(err) => {
                runtime_log_error(format!(
                    "[用户画像] 失败，任务=prepare_archive_active_conversation_seed_snapshot，agent_id={}，error={}",
                    agent.id,
                    err
                ));
                None
            }
        });
    if let Some(snapshot) = profile_snapshot {
        conversation.user_profile_snapshot = snapshot;
    }
    let summary_message = build_initial_summary_context_message(
        Some(source.summary.as_str()),
        option_str_or_none(conversation.user_profile_snapshot.as_str()),
        Some(&conversation.current_todos),
    );
    conversation.last_user_at = Some(summary_message.created_at.clone());
    conversation.updated_at = summary_message.created_at.clone();
    conversation.messages.push(summary_message);
    Ok(conversation)
}

fn option_str_or_none(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}
