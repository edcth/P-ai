impl ConversationService {
    fn list_remote_im_contact_conversations(
        &self,
        state: &AppState,
    ) -> Result<Vec<RemoteImContactConversationSummary>, String> {
        let guard = state
            .conversation_lock
            .lock()
            .map_err(|err| state_lock_error_with_panic(file!(), line!(), module_path!(), &err))?;
        let mut items = with_app_data_cached_ref(state, |data, _detail| {
            Ok(data
                .remote_im_contacts
                .iter()
                .filter_map(|contact| {
                    let conversation_id = contact
                        .bound_conversation_id
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToOwned::to_owned)
                        .or_else(|| {
                            self.find_remote_im_contact_conversation_id_in_data(data, contact)
                        })?;
                    let conversation = data.conversations.iter().find(|conversation| {
                        conversation.id == conversation_id
                            && conversation.summary.trim().is_empty()
                            && conversation_is_remote_im_contact(conversation)
                    })?;
                    Some(RemoteImContactConversationSummary {
                        contact_id: contact.id.clone(),
                        conversation_id: conversation.id.clone(),
                        title: remote_im_contact_conversation_title(contact),
                        updated_at: conversation.updated_at.clone(),
                        last_message_at: conversation
                            .messages
                            .last()
                            .map(|message| message.created_at.clone()),
                        message_count: conversation.messages.len(),
                        channel_id: contact.channel_id.clone(),
                        platform: contact.platform.clone(),
                        contact_display_name: remote_im_contact_display_name(contact),
                        bound_department_id: contact.bound_department_id.clone(),
                        processing_mode: normalize_contact_processing_mode(
                            &contact.processing_mode,
                        ),
                    })
                })
                .collect::<Vec<_>>())
        })?;
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
        let mut messages = with_app_data_cached_ref(state, |data, _detail| {
            let contact = data
                .remote_im_contacts
                .iter()
                .find(|item| item.id == normalized_contact_id)
                .ok_or_else(|| format!("未找到远程联系人：{normalized_contact_id}"))?;
            let conversation_id = contact
                .bound_conversation_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .or_else(|| self.find_remote_im_contact_conversation_id_in_data(data, contact))
                .ok_or_else(|| format!("联系人未绑定联系人会话：{normalized_contact_id}"))?;
            data.conversations
                .iter()
                .find(|conversation| {
                    conversation.id == conversation_id
                        && conversation.summary.trim().is_empty()
                        && conversation_is_remote_im_contact(conversation)
                })
                .map(|conversation| conversation.messages.clone())
                .ok_or_else(|| format!("联系人会话不存在：{conversation_id}"))
        })?;
        drop(guard);
        materialize_chat_message_parts_from_media_refs(&mut messages, &state.data_path);
        Ok(messages)
    }

    fn ensure_remote_im_contact_conversation_id(
        &self,
        data: &mut AppData,
        contact: &mut RemoteImContact,
    ) -> Result<String, String> {
        if let Some(found_id) = self.find_remote_im_contact_conversation_id_in_data(data, contact) {
            contact.bound_conversation_id = Some(found_id.clone());
            return Ok(found_id);
        }

        let mut conversation = build_conversation_record(
            "",
            "",
            "",
            &remote_im_contact_conversation_title(contact),
            CONVERSATION_KIND_REMOTE_IM_CONTACT,
            Some(remote_im_contact_conversation_key(contact)),
            None,
        );
        conversation.status = "inactive".to_string();
        let conversation_id = conversation.id.clone();
        data.conversations.push(conversation);
        contact.bound_conversation_id = Some(conversation_id.clone());
        Ok(conversation_id)
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
        let mut data = state_read_app_data_cached(state)?;
        let Some(contact_idx) = data
            .remote_im_contacts
            .iter()
            .position(|item| item.id == normalized_contact_id)
        else {
            drop(guard);
            return Err(format!("未找到远程联系人：{normalized_contact_id}"));
        };
        let conversation_id = {
            let contact = &data.remote_im_contacts[contact_idx];
            contact
                .bound_conversation_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .or_else(|| self.find_remote_im_contact_conversation_id_in_data(&data, contact))
        };
        let Some(conversation_id) = conversation_id else {
            drop(guard);
            return Ok(false);
        };
        let Some(conversation) = data.conversations.iter_mut().find(|conversation| {
            conversation.id == conversation_id
                && conversation.summary.trim().is_empty()
                && conversation_is_remote_im_contact(conversation)
        }) else {
            drop(guard);
            return Ok(false);
        };

        conversation.messages.clear();
        conversation.memory_recall_table.clear();
        conversation.last_user_at = None;
        conversation.last_assistant_at = None;
        conversation.status = "inactive".to_string();
        conversation.updated_at = now_iso();

        persist_single_conversation_runtime_fast(state, &data, &conversation_id)?;
        drop(guard);
        Ok(true)
    }
}
