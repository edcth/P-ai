#[derive(Debug, Clone)]
struct PreparedConversationPromptPayload {
    history_messages: Vec<PreparedHistoryMessage>,
    latest_user_text: String,
    latest_user_meta_text: String,
    latest_user_extra_blocks: Vec<String>,
    latest_images: Vec<PreparedBinaryPayload>,
    latest_audios: Vec<PreparedBinaryPayload>,
}

#[derive(Debug, Clone)]
struct DepartmentSystemPromptSnapshot {
    department_prompt_block: String,
    department_tool_rule_blocks: Vec<String>,
    system_prompt_text: String,
    rebuilt_at: String,
}

#[derive(Debug, Clone)]
struct ConversationEnvironmentPromptSnapshot {
    runtime_blocks: Vec<String>,
    im_rule_blocks: Vec<String>,
    system_prompt_text: String,
    rebuilt_at: String,
}

#[derive(Debug, Clone)]
struct SystemPromptCacheEntry {
    prompt_text: String,
    rebuilt_at: String,
}

fn department_system_prompt_cache(
) -> &'static Mutex<std::collections::HashMap<String, DepartmentSystemPromptSnapshot>> {
    static CACHE: OnceLock<
        Mutex<std::collections::HashMap<String, DepartmentSystemPromptSnapshot>>,
    > = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

fn system_prompt_cache() -> &'static Mutex<std::collections::HashMap<String, SystemPromptCacheEntry>>
{
    static CACHE: OnceLock<Mutex<std::collections::HashMap<String, SystemPromptCacheEntry>>> =
        OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

fn conversation_environment_prompt_cache(
) -> &'static Mutex<std::collections::HashMap<String, ConversationEnvironmentPromptSnapshot>> {
    static CACHE: OnceLock<
        Mutex<std::collections::HashMap<String, ConversationEnvironmentPromptSnapshot>>,
    > = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

fn cache_lock_recover<'a, T>(
    label: &str,
    mutex: &'a Mutex<T>,
) -> std::sync::MutexGuard<'a, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(err) => {
            runtime_log_info(format!(
                "[系统提示词] 警告: {} 锁已 poison，继续恢复使用 error={:?}",
                label, err
            ));
            err.into_inner()
        }
    }
}

fn prompt_cache_scope_key(state: Option<&AppState>) -> String {
    state
        .map(|value| value.data_path.display().to_string())
        .unwrap_or_else(|| "<global>".to_string())
}

fn department_permission_control_signature(control: &DepartmentPermissionControl) -> String {
    format!(
        "enabled={}|mode={}|builtin={}|skill={}|mcp={}",
        control.enabled,
        control.mode.trim(),
        control
            .builtin_tool_names
            .iter()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join(","),
        control
            .skill_names
            .iter()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join(","),
        control
            .mcp_tool_names
            .iter()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join(","),
    )
}

fn department_runtime_signature(department: &DepartmentConfig) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}",
        department.id.trim(),
        department.updated_at.trim(),
        department.name.trim(),
        department.summary.trim(),
        department.guide.trim(),
        department
            .agent_ids
            .iter()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join(","),
        department.order_index,
        department_permission_control_signature(&department.permission_control),
    )
}

fn departments_runtime_signature(departments: &[DepartmentConfig]) -> String {
    departments
        .iter()
        .map(department_runtime_signature)
        .collect::<Vec<_>>()
        .join("||")
}

fn conversation_department_context_signature(
    conversation: &Conversation,
    current_department: Option<&DepartmentConfig>,
) -> String {
    let Some(current_department) = current_department else {
        return "department_context=none".to_string();
    };
    let latest_user = conversation
        .messages
        .iter()
        .rev()
        .find(|message| prompt_role_for_message(message, &conversation.agent_id).as_deref() == Some("user"));
    let Some(meta) = latest_user
        .and_then(|message| message.provider_meta.as_ref())
        .and_then(Value::as_object)
    else {
        return format!(
            "department_context=default|target={}|call_stack=",
            current_department.id.trim()
        );
    };
    let target_department_id = meta
        .get("targetDepartmentId")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    let mut call_stack = meta
        .get("callStack")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    call_stack.sort();
    format!(
        "department_context=provider_meta|target={}|current={}|call_stack={}",
        target_department_id,
        current_department.id.trim(),
        call_stack.join(","),
    )
}

fn build_department_system_prompt_cache_key(
    state: Option<&AppState>,
    conversation: &Conversation,
    agent: &AgentProfile,
    departments: &[DepartmentConfig],
    ui_language: &str,
) -> String {
    let config = departments_only_config(departments);
    let current_department = department_for_agent_id(&config, &agent.id);
    format!(
        "scope={}|agent={}|ui={}|departments={}|context={}",
        prompt_cache_scope_key(state),
        agent.id.trim(),
        ui_language.trim(),
        departments_runtime_signature(departments),
        conversation_department_context_signature(conversation, current_department),
    )
}

fn build_department_system_prompt_snapshot_uncached(
    _state: Option<&AppState>,
    conversation: &Conversation,
    agent: &AgentProfile,
    departments: &[DepartmentConfig],
    ui_language: &str,
) -> DepartmentSystemPromptSnapshot {
    let department_prompt_block =
        build_departments_prompt_block(conversation, agent, departments, ui_language);
    let department_tool_rule_blocks = build_system_tools_rule_blocks(agent, departments);
    let mut sections = Vec::<String>::new();
    if !department_prompt_block.trim().is_empty() {
        sections.push(department_prompt_block.trim().to_string());
    }
    for block in &department_tool_rule_blocks {
        let trimmed = block.trim();
        if !trimmed.is_empty() {
            sections.push(trimmed.to_string());
        }
    }
    let system_prompt_text = sections.join("\n");
    DepartmentSystemPromptSnapshot {
        department_prompt_block,
        department_tool_rule_blocks,
        system_prompt_text,
        rebuilt_at: now_iso(),
    }
}

fn get_or_build_department_system_prompt_snapshot(
    state: Option<&AppState>,
    conversation: &Conversation,
    agent: &AgentProfile,
    departments: &[DepartmentConfig],
    ui_language: &str,
) -> DepartmentSystemPromptSnapshot {
    let cache_key =
        build_department_system_prompt_cache_key(state, conversation, agent, departments, ui_language);
    {
        let cache = cache_lock_recover("department_system_prompt_cache", department_system_prompt_cache());
        if let Some(entry) = cache.get(&cache_key) {
            runtime_log_info(format!(
                "[部门提示词] 命中缓存 department_id={} rebuilt_at={}",
                department_for_agent_id(&departments_only_config(departments), &agent.id)
                    .map(|item| item.id.trim().to_string())
                    .unwrap_or_default(),
                entry.rebuilt_at
            ));
            return entry.clone();
        }
    }
    runtime_log_info(format!(
        "[部门提示词] 开始重建 department_id={} reason=cache_miss",
        department_for_agent_id(&departments_only_config(departments), &agent.id)
            .map(|item| item.id.trim().to_string())
            .unwrap_or_default()
    ));
    let snapshot = build_department_system_prompt_snapshot_uncached(
        state,
        conversation,
        agent,
        departments,
        ui_language,
    );
    runtime_log_info(format!(
        "[部门提示词] 重建完成 department_id={} chars={}",
        department_for_agent_id(&departments_only_config(departments), &agent.id)
            .map(|item| item.id.trim().to_string())
            .unwrap_or_default(),
        snapshot.system_prompt_text.chars().count()
    ));
    let mut cache = cache_lock_recover("department_system_prompt_cache", department_system_prompt_cache());
    cache.insert(cache_key, snapshot.clone());
    snapshot
}

fn conversation_shell_runtime_signature(state: Option<&AppState>) -> String {
    let Some(state) = state else {
        return "shell=no_state".to_string();
    };
    let shell = terminal_shell_for_state(state);
    format!("shell={}|path={}", shell.kind.trim(), shell.path.trim())
}

fn conversation_workspace_runtime_signature(conversation: &Conversation) -> String {
    let root = conversation
        .shell_workspace_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("");
    let workspaces = conversation
        .shell_workspaces
        .iter()
        .map(|workspace| {
            format!(
                "{}:{}:{}:{}:{}:{}",
                workspace.id.trim(),
                workspace.path.trim(),
                workspace.level.trim(),
                workspace.access.trim(),
                workspace.name.trim(),
                workspace.built_in
            )
        })
        .collect::<Vec<_>>()
        .join("|");
    format!(
        "conversation_id={}|kind={}|root={}|workspaces={}",
        conversation.id.trim(),
        conversation.conversation_kind.trim(),
        root,
        workspaces
    )
}

fn split_system_preamble_blocks(
    system_preamble_blocks: &[String],
) -> (Vec<String>, Vec<String>, Vec<String>) {
    let mut tool_rule_blocks = Vec::<String>::new();
    let mut runtime_blocks = Vec::<String>::new();
    let mut im_rule_blocks = Vec::<String>::new();
    for block in system_preamble_blocks {
        let trimmed = block.trim();
        if trimmed.is_empty() {
            continue;
        }
        match classify_system_prompt_extra_block(trimmed) {
            SystemPromptExtraBlockGroup::ToolRules => tool_rule_blocks.push(trimmed.to_string()),
            SystemPromptExtraBlockGroup::Runtime => runtime_blocks.push(trimmed.to_string()),
            SystemPromptExtraBlockGroup::ImRules => im_rule_blocks.push(trimmed.to_string()),
        }
    }
    (tool_rule_blocks, runtime_blocks, im_rule_blocks)
}

fn build_conversation_environment_prompt_cache_key(
    state: Option<&AppState>,
    conversation: &Conversation,
    ui_language: &str,
    terminal_block: Option<&str>,
    runtime_extra_blocks: &[String],
    im_extra_blocks: &[String],
) -> String {
    format!(
        "scope={}|ui={}|{}|{}|terminal={}|runtime_extra={}|im_extra={}|remote_contact={}",
        prompt_cache_scope_key(state),
        ui_language.trim(),
        conversation_shell_runtime_signature(state),
        conversation_workspace_runtime_signature(conversation),
        terminal_block.map(str::trim).unwrap_or(""),
        runtime_extra_blocks
            .iter()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join("\n<runtime>\n"),
        im_extra_blocks
            .iter()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join("\n<im>\n"),
        conversation_is_remote_im_contact(conversation)
    )
}

fn build_conversation_environment_prompt_snapshot_uncached(
    conversation: &Conversation,
    terminal_block: Option<&str>,
    runtime_extra_blocks: &[String],
    im_extra_blocks: &[String],
) -> ConversationEnvironmentPromptSnapshot {
    let mut runtime_blocks = Vec::<String>::new();
    if let Some(terminal_block) = terminal_block
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        runtime_blocks.push(terminal_block.to_string());
    }
    runtime_blocks.extend(
        runtime_extra_blocks
            .iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
    );

    let mut im_rule_blocks = Vec::<String>::new();
    if conversation_is_remote_im_contact(conversation) {
        im_rule_blocks.push(prompt_xml_block(
            "remote im contact rules",
            "联系人是特殊用户，不是当前聊天窗口中的直接用户。\n他们的消息来自远程接口接入，应视为独立的外部用户。\n不要把联系人和当前用户混为一谈，也不要混淆回复目标。\n如果需要回复远程联系人，必须调用 `remote_im_send`。",
        ));
    }
    im_rule_blocks.extend(
        im_extra_blocks
            .iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
    );

    let mut sections = Vec::<String>::new();
    sections.extend(runtime_blocks.iter().cloned());
    sections.extend(im_rule_blocks.iter().cloned());
    let system_prompt_text = sections.join("\n");
    ConversationEnvironmentPromptSnapshot {
        runtime_blocks,
        im_rule_blocks,
        system_prompt_text,
        rebuilt_at: now_iso(),
    }
}

fn get_or_build_conversation_environment_prompt_snapshot(
    state: Option<&AppState>,
    conversation: &Conversation,
    ui_language: &str,
    terminal_block: Option<&str>,
    runtime_extra_blocks: &[String],
    im_extra_blocks: &[String],
) -> ConversationEnvironmentPromptSnapshot {
    let cache_key = build_conversation_environment_prompt_cache_key(
        state,
        conversation,
        ui_language,
        terminal_block,
        runtime_extra_blocks,
        im_extra_blocks,
    );
    {
        let cache = cache_lock_recover(
            "conversation_environment_prompt_cache",
            conversation_environment_prompt_cache(),
        );
        if let Some(entry) = cache.get(&cache_key) {
            runtime_log_info(format!(
                "[会话环境提示词] 命中缓存 conversation_id={} rebuilt_at={}",
                conversation.id.trim(),
                entry.rebuilt_at
            ));
            return entry.clone();
        }
    }
    runtime_log_info(format!(
        "[会话环境提示词] 开始重建 conversation_id={} reason=cache_miss",
        conversation.id.trim()
    ));
    let snapshot = build_conversation_environment_prompt_snapshot_uncached(
        conversation,
        terminal_block,
        runtime_extra_blocks,
        im_extra_blocks,
    );
    runtime_log_info(format!(
        "[会话环境提示词] 重建完成 conversation_id={} chars={}",
        conversation.id.trim(),
        snapshot.system_prompt_text.chars().count()
    ));
    let mut cache = cache_lock_recover(
        "conversation_environment_prompt_cache",
        conversation_environment_prompt_cache(),
    );
    cache.insert(cache_key, snapshot.clone());
    snapshot
}

fn append_system_prompt_block(target: &mut String, block: Option<&str>) {
    let Some(trimmed) = block.map(str::trim).filter(|value| !value.is_empty()) else {
        return;
    };
    if !target.trim().is_empty() {
        if !target.ends_with('\n') {
            target.push('\n');
        }
    }
    target.push_str(trimmed);
    target.push('\n');
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SystemPromptExtraBlockGroup {
    ToolRules,
    Runtime,
    ImRules,
}

fn classify_system_prompt_extra_block(block: &str) -> SystemPromptExtraBlockGroup {
    let trimmed = block.trim();
    if trimmed.contains("<remote im runtime activation>") {
        return SystemPromptExtraBlockGroup::ImRules;
    }
    if trimmed.contains("<skill usage>")
        || trimmed.contains("<skill index>")
        || trimmed.contains("<todo guide>")
    {
        return SystemPromptExtraBlockGroup::ToolRules;
    }
    SystemPromptExtraBlockGroup::Runtime
}

fn build_core_system_prompt_text(
    _conversation: &Conversation,
    agent: &AgentProfile,
    _departments: &[DepartmentConfig],
    user_profile: Option<(&str, &str)>,
    response_style_id: &str,
    ui_language: &str,
    _state: Option<&AppState>,
) -> String {
    let response_style = response_style_preset(response_style_id);
    let date_timezone_line = prompt_current_date_timezone_line(ui_language);
    let highest_instruction_md = highest_instruction_markdown();
    let (
        not_provided_label,
        assistant_settings_label,
        user_settings_label,
        role_constraints_label,
        conversation_style_label,
        language_settings_label,
        user_nickname_label,
        user_intro_label,
        role_identity_line,
        role_confusion_line,
        language_follow_user_line,
        language_instruction,
    ) = (
        "未提供",
        "persona settings",
        "admin user settings",
        "role constraints",
        "conversation style",
        "language settings",
        "用户昵称",
        "用户自我介绍",
        "- 你是“{}”，用户是“{}”。",
        "- 不要把自己当作用户，不要混淆双方身份。",
        "- 若用户明确指定回答语言，以用户指定为准。",
        "默认使用中文回答。",
    );
    if let Some((user_name, user_intro)) = user_profile {
        let user_intro_display = if user_intro.trim().is_empty() {
            not_provided_label.to_string()
        } else {
            user_intro.trim().to_string()
        };
        let role_identity_text = role_identity_line
            .replacen("{}", &xml_escape_prompt(&agent.name), 1)
            .replacen("{}", &xml_escape_prompt(user_name), 1);
        [
            highest_instruction_md.to_string(),
            prompt_xml_block(assistant_settings_label, agent.system_prompt.trim()),
            prompt_xml_block(
                user_settings_label,
                format!(
                    "{}：{}\n{}：{}",
                    user_nickname_label,
                    xml_escape_prompt(user_name),
                    user_intro_label,
                    xml_escape_prompt(&user_intro_display)
                ),
            ),
            prompt_xml_block(
                role_constraints_label,
                format!("{}\n{}", role_identity_text, role_confusion_line),
            ),
            prompt_xml_block(
                conversation_style_label,
                format!("当前风格：{}\n{}", response_style.name, response_style.prompt),
            ),
            prompt_xml_block(
                language_settings_label,
                format!(
                    "{}\n{}\n{}",
                    language_instruction, language_follow_user_line, date_timezone_line
                ),
            ),
        ]
        .join("\n")
    } else {
        let delegate_role_line = "- 这是一条委托线程。此线程不存在默认用户人格。";
        let delegate_scope_line =
            "- 只依据本轮委托任务块与本线程历史处理工作，不要自行补充用户设定、昵称或主会话背景。";
        [
            highest_instruction_md.to_string(),
            prompt_xml_block(assistant_settings_label, agent.system_prompt.trim()),
            prompt_xml_block(
                role_constraints_label,
                format!("{}\n{}", delegate_role_line, delegate_scope_line),
            ),
            prompt_xml_block(
                conversation_style_label,
                format!("当前风格：{}\n{}", response_style.name, response_style.prompt),
            ),
            prompt_xml_block(
                language_settings_label,
                format!("{}\n{}", language_instruction, date_timezone_line),
            ),
        ]
        .join("\n")
    }
}

fn build_system_prompt_cache_key(
    state: Option<&AppState>,
    mode_label: &str,
    agent: &AgentProfile,
    ordered_blocks: &[String],
) -> String {
    format!(
        "scope={}|mode={}|agent={}|ordered_blocks={}",
        prompt_cache_scope_key(state),
        mode_label.trim(),
        agent.id.trim(),
        ordered_blocks
            .iter()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join("\n<block-sep>\n"),
    )
}

fn build_system_prompt_ordered_blocks(
    state: Option<&AppState>,
    conversation: &Conversation,
    agent: &AgentProfile,
    departments: &[DepartmentConfig],
    ui_language: &str,
    _selected_api: Option<&ApiConfig>,
    fixed_system_prompt_text: &str,
    user_profile_memory_block: Option<&str>,
    terminal_block: Option<&str>,
    system_preamble_blocks: &[String],
) -> Vec<String> {
    let department_snapshot = get_or_build_department_system_prompt_snapshot(
        state,
        conversation,
        agent,
        departments,
        ui_language,
    );
    let department_config = departments_only_config(departments);
    let current_department = department_for_agent_id(&department_config, &agent.id);
    let mut tool_rule_blocks = Vec::<String>::new();
    if ["remember", "recall"]
        .into_iter()
        .any(|tool_id| department_builtin_tool_enabled(current_department, tool_id))
    {
        tool_rule_blocks.push(build_memory_rag_rule_block());
    }
    tool_rule_blocks.extend(department_snapshot.department_tool_rule_blocks.iter().cloned());
    if department_builtin_tool_enabled(current_department, "plan") {
        tool_rule_blocks.push(build_question_and_planning_rule_block());
    }
    if department_builtin_tool_enabled(current_department, "meme") {
        if let Some(meme_block) = meme_prompt_rule_block(state).as_deref() {
            tool_rule_blocks.push(meme_block.trim().to_string());
        }
    }

    let (tool_rule_extra_blocks, runtime_extra_blocks, im_extra_blocks) =
        split_system_preamble_blocks(system_preamble_blocks);
    tool_rule_blocks.extend(tool_rule_extra_blocks);

    let environment_snapshot = get_or_build_conversation_environment_prompt_snapshot(
        state,
        conversation,
        ui_language,
        terminal_block,
        &runtime_extra_blocks,
        &im_extra_blocks,
    );

    let mut ordered_blocks = Vec::<String>::new();
    if !fixed_system_prompt_text.trim().is_empty() {
        ordered_blocks.push(fixed_system_prompt_text.trim().to_string());
    }
    if !department_snapshot.department_prompt_block.trim().is_empty() {
        ordered_blocks.push(department_snapshot.department_prompt_block.trim().to_string());
    }
    ordered_blocks.extend(
        tool_rule_blocks
            .into_iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
    );
    if let Some(profile_block) = user_profile_memory_block
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        ordered_blocks.push(profile_block.to_string());
    }
    ordered_blocks.extend(
        environment_snapshot
            .runtime_blocks
            .into_iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
    );
    ordered_blocks.extend(
        environment_snapshot
            .im_rule_blocks
            .into_iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
    );
    ordered_blocks
}

fn build_system_prompt_text_uncached(ordered_blocks: &[String]) -> String {
    let mut prompt = String::new();
    for block in ordered_blocks {
        append_system_prompt_block(&mut prompt, Some(block));
    }
    prompt
}

fn finalize_system_prompt_with_manager(
    state: Option<&AppState>,
    mode_label: &str,
    conversation: &Conversation,
    agent: &AgentProfile,
    departments: &[DepartmentConfig],
    selected_api: Option<&ApiConfig>,
    _user_profile: Option<(&str, &str)>,
    _response_style_id: &str,
    ui_language: &str,
    fixed_system_prompt_text: &str,
    user_profile_memory_block: Option<&str>,
    terminal_block: Option<&str>,
    system_preamble_blocks: &[String],
) -> String {
    let ordered_blocks = build_system_prompt_ordered_blocks(
        state,
        conversation,
        agent,
        departments,
        ui_language,
        selected_api,
        fixed_system_prompt_text,
        user_profile_memory_block,
        terminal_block,
        system_preamble_blocks,
    );
    let cache_key = build_system_prompt_cache_key(state, mode_label, agent, &ordered_blocks);
    {
        let cache = cache_lock_recover("system_prompt_cache", system_prompt_cache());
        if let Some(entry) = cache.get(&cache_key) {
            runtime_log_info(format!(
                "[系统提示词] 命中缓存 department_id={} rebuilt_at={}",
                department_for_agent_id(&departments_only_config(departments), &agent.id)
                    .map(|item| item.id.trim().to_string())
                    .unwrap_or_default(),
                entry.rebuilt_at
            ));
            return entry.prompt_text.clone();
        }
    }
    runtime_log_info(format!(
        "[系统提示词] 开始构建 department_id={} mode={} conversation_id={}",
        department_for_agent_id(&departments_only_config(departments), &agent.id)
            .map(|item| item.id.trim().to_string())
            .unwrap_or_default(),
        mode_label.trim(),
        conversation.id.trim()
    ));
    let prompt_text = build_system_prompt_text_uncached(&ordered_blocks);
    runtime_log_info(format!(
        "[系统提示词] 完成构建 department_id={} mode={} chars={}",
        department_for_agent_id(&departments_only_config(departments), &agent.id)
            .map(|item| item.id.trim().to_string())
            .unwrap_or_default(),
        mode_label.trim(),
        prompt_text.chars().count()
    ));
    let entry = SystemPromptCacheEntry {
        prompt_text: prompt_text.clone(),
        rebuilt_at: now_iso(),
    };
    let mut cache = cache_lock_recover("system_prompt_cache", system_prompt_cache());
    cache.insert(cache_key, entry);
    prompt_text
}
