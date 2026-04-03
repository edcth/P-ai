    #[test]
    fn image_text_cache_upsert_and_find_should_work() {
        let mut data = AppData::default();
        upsert_image_text_cache(&mut data, "h1", "vision-a", "text-a");
        assert_eq!(
            find_image_text_cache(&data, "h1", "vision-a"),
            Some("text-a".to_string())
        );

        upsert_image_text_cache(&mut data, "h1", "vision-a", "text-b");
        assert_eq!(
            find_image_text_cache(&data, "h1", "vision-a"),
            Some("text-b".to_string())
        );
        assert_eq!(find_image_text_cache(&data, "h1", "vision-b"), None);
    }

    #[test]
    fn compute_image_hash_hex_should_be_stable() {
        let png_1x1_red = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO9WfXkAAAAASUVORK5CYII=";
        let part = BinaryPart {
            mime: "image/png".to_string(),
            bytes_base64: png_1x1_red.to_string(),
            saved_path: None,
        };
        let h1 = compute_image_hash_hex(&part).expect("hash1");
        let h2 = compute_image_hash_hex(&part).expect("hash2");
        assert_eq!(h1, h2);
        assert!(!h1.is_empty());
    }

    #[test]
    fn normalize_app_config_should_fix_invalid_record_and_stt_fields() {
        let mut cfg = AppConfig {
            hotkey: "Alt+·".to_string(),
            ui_language: default_ui_language(),
            ui_font: default_ui_font(),
            record_hotkey: "".to_string(),
            record_background_wake_enabled: false,
            min_record_seconds: 0,
            max_record_seconds: 0,
            tool_max_iterations: 0,
            selected_api_config_id: "a1".to_string(),
            assistant_department_api_config_id: "a1".to_string(),
            vision_api_config_id: None,
            stt_api_config_id: None,
            stt_auto_send: false,
            provider_non_stream_base_urls: Vec::new(),
            terminal_shell_kind: default_terminal_shell_kind(),
            shell_workspaces: Vec::new(),
            mcp_servers: Vec::new(),
            remote_im_channels: Vec::new(),
            departments: Vec::new(),
            api_configs: vec![
                ApiConfig {
                    id: "a1".to_string(),
                    name: "chat".to_string(),
                    request_format: RequestFormat::OpenAI,
                    enable_text: true,
                    enable_image: true,
                    enable_audio: false,
                    enable_tools: false,
                    tools: vec![],
                    base_url: "https://api.openai.com/v1".to_string(),
                    api_key: "k".to_string(),
                    model: "m".to_string(),
                    temperature: 1.0,
                    custom_temperature_enabled: false,
                    context_window_tokens: 128_000,
                    max_output_tokens: 4_096,
                    custom_max_output_tokens_enabled: false,
                    failure_retry_count: 999,
                },
                ApiConfig {
                    id: "a2".to_string(),
                    name: "bad-stt".to_string(),
                    request_format: RequestFormat::OpenAI,
                    enable_text: true,
                    enable_image: false,
                    enable_audio: true,
                    enable_tools: false,
                    tools: vec![],
                    base_url: "https://api.openai.com/v1".to_string(),
                    api_key: "k".to_string(),
                    model: "m".to_string(),
                    temperature: 1.0,
                    custom_temperature_enabled: false,
                    context_window_tokens: 128_000,
                    max_output_tokens: 4_096,
                    custom_max_output_tokens_enabled: false,
                    failure_retry_count: 0,
                },
            ],
            api_providers: Vec::new(),
        };
        normalize_app_config(&mut cfg);
        assert_eq!(cfg.record_hotkey, "Alt");
        assert_eq!(cfg.min_record_seconds, 1);
        assert!(cfg.max_record_seconds >= cfg.min_record_seconds);
        assert_eq!(cfg.tool_max_iterations, 1);
        assert_eq!(cfg.api_configs[0].failure_retry_count, 20);
        assert!(!cfg.stt_auto_send);
    }

    #[test]
    fn normalize_app_config_should_not_bind_chat_api_to_selected_api() {
        let mut cfg = AppConfig {
            hotkey: "Alt+·".to_string(),
            ui_language: default_ui_language(),
            ui_font: default_ui_font(),
            record_hotkey: "Alt".to_string(),
            record_background_wake_enabled: false,
            min_record_seconds: 1,
            max_record_seconds: 60,
            tool_max_iterations: 10,
            selected_api_config_id: "edit-b".to_string(),
            assistant_department_api_config_id: "chat-a".to_string(),
            vision_api_config_id: None,
            stt_api_config_id: None,
            stt_auto_send: false,
            provider_non_stream_base_urls: Vec::new(),
            terminal_shell_kind: default_terminal_shell_kind(),
            shell_workspaces: Vec::new(),
            mcp_servers: Vec::new(),
            remote_im_channels: Vec::new(),
            departments: Vec::new(),
            api_configs: vec![
                ApiConfig {
                    id: "chat-a".to_string(),
                    name: "chat-a".to_string(),
                    request_format: RequestFormat::OpenAI,
                    enable_text: true,
                    enable_image: true,
                    enable_audio: true,
                    enable_tools: false,
                    tools: vec![],
                    base_url: "https://api.openai.com/v1".to_string(),
                    api_key: "k".to_string(),
                    model: "m".to_string(),
                    temperature: 1.0,
                    custom_temperature_enabled: false,
                    context_window_tokens: 128_000,
                    max_output_tokens: 4_096,
                    custom_max_output_tokens_enabled: false,
                    failure_retry_count: 0,
                },
                ApiConfig {
                    id: "edit-b".to_string(),
                    name: "edit-b".to_string(),
                    request_format: RequestFormat::OpenAI,
                    enable_text: true,
                    enable_image: false,
                    enable_audio: false,
                    enable_tools: false,
                    tools: vec![],
                    base_url: "https://api.openai.com/v1".to_string(),
                    api_key: "k".to_string(),
                    model: "m".to_string(),
                    temperature: 1.0,
                    custom_temperature_enabled: false,
                    context_window_tokens: 128_000,
                    max_output_tokens: 4_096,
                    custom_max_output_tokens_enabled: false,
                    failure_retry_count: 0,
                },
            ],
            api_providers: Vec::new(),
        };
        normalize_app_config(&mut cfg);
        assert_eq!(cfg.selected_api_config_id, "edit-b::edit-b-model-default".to_string());
        assert_eq!(
            cfg.assistant_department_api_config_id,
            "chat-a::chat-a-model-default".to_string()
        );
    }

    #[test]
    fn normalize_app_config_should_disable_audio_capability_globally() {
        let mut cfg = AppConfig {
            hotkey: "Alt+·".to_string(),
            ui_language: default_ui_language(),
            ui_font: default_ui_font(),
            record_hotkey: "Alt".to_string(),
            record_background_wake_enabled: false,
            min_record_seconds: 1,
            max_record_seconds: 60,
            tool_max_iterations: 10,
            selected_api_config_id: "tts-a".to_string(),
            assistant_department_api_config_id: "tts-a".to_string(),
            vision_api_config_id: Some("tts-a".to_string()),
            stt_api_config_id: Some("tts-a".to_string()),
            stt_auto_send: true,
            provider_non_stream_base_urls: Vec::new(),
            terminal_shell_kind: default_terminal_shell_kind(),
            shell_workspaces: Vec::new(),
            mcp_servers: Vec::new(),
            remote_im_channels: Vec::new(),
            departments: Vec::new(),
            api_configs: vec![ApiConfig {
                id: "tts-a".to_string(),
                name: "tts-a".to_string(),
                request_format: RequestFormat::OpenAITts,
                enable_text: true,
                enable_image: false,
                enable_audio: true,
                enable_tools: true,
                tools: vec![],
                base_url: "https://api.siliconflow.cn/v1/audio/transcriptions".to_string(),
                api_key: "k".to_string(),
                model: "m".to_string(),
                temperature: 1.0,
                custom_temperature_enabled: false,
                context_window_tokens: 128_000,
                max_output_tokens: 4_096,
                custom_max_output_tokens_enabled: false,
                failure_retry_count: 0,
            }],
            api_providers: Vec::new(),
        };
        normalize_app_config(&mut cfg);
        let api = &cfg.api_configs[0];
        assert!(api.enable_text);
        assert!(!api.enable_image);
        assert!(!api.enable_audio);
        assert!(api.enable_tools);
        assert_eq!(cfg.vision_api_config_id, None);
        assert!(cfg.stt_auto_send);
    }

    #[test]
    fn normalize_app_config_should_drop_invalid_department_models_without_frontend_fallback() {
        let mut cfg = AppConfig {
            hotkey: "Alt+·".to_string(),
            ui_language: default_ui_language(),
            ui_font: default_ui_font(),
            record_hotkey: "Alt".to_string(),
            record_background_wake_enabled: false,
            min_record_seconds: 1,
            max_record_seconds: 60,
            tool_max_iterations: 10,
            selected_api_config_id: "embed-a".to_string(),
            assistant_department_api_config_id: "chat-a".to_string(),
            vision_api_config_id: None,
            stt_api_config_id: None,
            stt_auto_send: false,
            provider_non_stream_base_urls: Vec::new(),
            terminal_shell_kind: default_terminal_shell_kind(),
            shell_workspaces: Vec::new(),
            mcp_servers: Vec::new(),
            remote_im_channels: Vec::new(),
            departments: vec![
                DepartmentConfig {
                    id: ASSISTANT_DEPARTMENT_ID.to_string(),
                    name: "助理部门".to_string(),
                    summary: String::new(),
                    guide: String::new(),
                    api_config_ids: vec!["embed-a".to_string()],
                    api_config_id: "embed-a".to_string(),
                    agent_ids: vec![DEFAULT_AGENT_ID.to_string()],
                    created_at: "2026-03-10T00:00:00Z".to_string(),
                    updated_at: "2026-03-10T00:00:00Z".to_string(),
                    order_index: 1,
                    is_built_in_assistant: true,
                    source: default_main_source(),
                    scope: default_global_scope(),
                },
                DepartmentConfig {
                    id: "department-research".to_string(),
                    name: "资料部".to_string(),
                    summary: String::new(),
                    guide: String::new(),
                    api_config_ids: vec!["stt-a".to_string()],
                    api_config_id: "stt-a".to_string(),
                    agent_ids: vec![],
                    created_at: "2026-03-10T00:00:00Z".to_string(),
                    updated_at: "2026-03-10T00:00:00Z".to_string(),
                    order_index: 2,
                    is_built_in_assistant: false,
                    source: default_main_source(),
                    scope: default_global_scope(),
                },
            ],
            api_configs: vec![
                ApiConfig {
                    id: "embed-a".to_string(),
                    name: "embed-a".to_string(),
                    request_format: RequestFormat::OpenAIEmbedding,
                    enable_text: true,
                    enable_image: false,
                    enable_audio: false,
                    enable_tools: false,
                    tools: vec![],
                    base_url: "https://api.openai.com/v1".to_string(),
                    api_key: "k".to_string(),
                    model: "embed".to_string(),
                    temperature: 1.0,
                    custom_temperature_enabled: false,
                    context_window_tokens: 128_000,
                    max_output_tokens: 4_096,
                    custom_max_output_tokens_enabled: false,
                    failure_retry_count: 0,
                },
                ApiConfig {
                    id: "stt-a".to_string(),
                    name: "stt-a".to_string(),
                    request_format: RequestFormat::OpenAIStt,
                    enable_text: false,
                    enable_image: false,
                    enable_audio: false,
                    enable_tools: false,
                    tools: vec![],
                    base_url: "https://api.openai.com/v1".to_string(),
                    api_key: "k".to_string(),
                    model: "stt".to_string(),
                    temperature: 1.0,
                    custom_temperature_enabled: false,
                    context_window_tokens: 128_000,
                    max_output_tokens: 4_096,
                    custom_max_output_tokens_enabled: false,
                    failure_retry_count: 0,
                },
                ApiConfig {
                    id: "chat-a".to_string(),
                    name: "chat-a".to_string(),
                    request_format: RequestFormat::OpenAI,
                    enable_text: true,
                    enable_image: true,
                    enable_audio: false,
                    enable_tools: false,
                    tools: vec![],
                    base_url: "https://api.openai.com/v1".to_string(),
                    api_key: "k".to_string(),
                    model: "chat".to_string(),
                    temperature: 1.0,
                    custom_temperature_enabled: false,
                    context_window_tokens: 128_000,
                    max_output_tokens: 4_096,
                    custom_max_output_tokens_enabled: false,
                    failure_retry_count: 0,
                },
            ],
            api_providers: Vec::new(),
        };

        normalize_app_config(&mut cfg);

        assert_eq!(cfg.assistant_department_api_config_id, "");
        let assistant = cfg
            .departments
            .iter()
            .find(|item| item.id == ASSISTANT_DEPARTMENT_ID)
            .expect("assistant department");
        assert_eq!(assistant.api_config_id, "");
        assert!(assistant.api_config_ids.is_empty());
        let research = cfg
            .departments
            .iter()
            .find(|item| item.id == "department-research")
            .expect("research department");
        assert_eq!(research.api_config_id, "");
        assert!(research.api_config_ids.is_empty());
    }

    #[test]
    fn normalize_app_config_should_preserve_empty_assistant_department_model() {
        let mut cfg = AppConfig {
            hotkey: "Alt+·".to_string(),
            ui_language: default_ui_language(),
            ui_font: default_ui_font(),
            record_hotkey: "Alt".to_string(),
            record_background_wake_enabled: false,
            min_record_seconds: 1,
            max_record_seconds: 60,
            tool_max_iterations: 10,
            selected_api_config_id: "chat-a".to_string(),
            assistant_department_api_config_id: String::new(),
            vision_api_config_id: None,
            stt_api_config_id: None,
            stt_auto_send: false,
            provider_non_stream_base_urls: Vec::new(),
            terminal_shell_kind: default_terminal_shell_kind(),
            shell_workspaces: Vec::new(),
            mcp_servers: Vec::new(),
            remote_im_channels: Vec::new(),
            departments: vec![DepartmentConfig {
                id: ASSISTANT_DEPARTMENT_ID.to_string(),
                name: "助理部门".to_string(),
                summary: String::new(),
                guide: String::new(),
                api_config_ids: Vec::new(),
                api_config_id: String::new(),
                agent_ids: vec![DEFAULT_AGENT_ID.to_string()],
                created_at: "2026-03-10T00:00:00Z".to_string(),
                updated_at: "2026-03-10T00:00:00Z".to_string(),
                order_index: 1,
                is_built_in_assistant: true,
                source: default_main_source(),
                scope: default_global_scope(),
            }],
            api_configs: vec![ApiConfig {
                id: "chat-a".to_string(),
                name: "chat-a".to_string(),
                request_format: RequestFormat::OpenAI,
                enable_text: true,
                enable_image: true,
                enable_audio: false,
                enable_tools: false,
                tools: vec![],
                base_url: "https://api.openai.com/v1".to_string(),
                api_key: "k".to_string(),
                model: "chat".to_string(),
                temperature: 1.0,
                custom_temperature_enabled: false,
                context_window_tokens: 128_000,
                max_output_tokens: 4_096,
                custom_max_output_tokens_enabled: false,
                failure_retry_count: 0,
            }],
            api_providers: Vec::new(),
        };

        normalize_app_config(&mut cfg);

        assert_eq!(cfg.assistant_department_api_config_id, "");
        assert_eq!(cfg.departments[0].api_config_id, "");
        assert!(cfg.departments[0].api_config_ids.is_empty());
    }

    #[test]
    fn normalize_terminal_path_input_should_strip_wrapping_quotes() {
        let out = normalize_terminal_path_input_for_current_platform(r#""./repo""#);
        assert_eq!(out, "./repo".to_string());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn normalize_terminal_path_input_should_convert_git_bash_style_on_windows() {
        let out = normalize_terminal_path_input_for_current_platform("/e/work/repo");
        assert_eq!(out, r"E:\work\repo".to_string());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn normalize_shell_workspaces_should_convert_and_dedup_windows_paths() {
        let mut cfg = AppConfig::default();
        cfg.shell_workspaces = vec![
            ShellWorkspaceConfig {
                name: "A".to_string(),
                path: "/e/__easy_call_ai_path_norm_test__/repo".to_string(),
                built_in: false,
            },
            ShellWorkspaceConfig {
                name: "a".to_string(),
                path: "E:/__easy_call_ai_path_norm_test__/repo".to_string(),
                built_in: false,
            },
            ShellWorkspaceConfig {
                name: "B".to_string(),
                path: r#""E:\__easy_call_ai_path_norm_test__\repo""#.to_string(),
                built_in: false,
            },
        ];
        normalize_shell_workspaces(&mut cfg);
        assert_eq!(cfg.shell_workspaces.len(), 2);
        assert_eq!(
            cfg.shell_workspaces[0].path,
            r"E:\__easy_call_ai_path_norm_test__\repo".to_string()
        );
    }

    #[test]
    fn normalize_app_config_should_migrate_legacy_api_configs_into_providers() {
        let mut cfg = AppConfig {
            selected_api_config_id: "legacy-openai".to_string(),
            assistant_department_api_config_id: "legacy-openai".to_string(),
            api_providers: Vec::new(),
            api_configs: vec![ApiConfig {
                id: "legacy-openai".to_string(),
                name: "Legacy OpenAI".to_string(),
                request_format: RequestFormat::OpenAI,
                enable_text: true,
                enable_image: false,
                enable_audio: false,
                enable_tools: true,
                tools: default_api_tools(),
                base_url: "https://api.openai.com/v1".to_string(),
                api_key: "legacy-key".to_string(),
                model: "gpt-4.1".to_string(),
                temperature: 0.7,
                custom_temperature_enabled: true,
                context_window_tokens: 256_000,
                max_output_tokens: 8_192,
                custom_max_output_tokens_enabled: true,
                failure_retry_count: 2,
            }],
            ..AppConfig::default()
        };

        normalize_app_config(&mut cfg);

        assert_eq!(cfg.api_providers.len(), 1);
        assert_eq!(cfg.api_providers[0].api_keys, vec!["legacy-key".to_string()]);
        assert_eq!(cfg.api_providers[0].models.len(), 1);
        assert_eq!(cfg.api_providers[0].models[0].model, "gpt-4.1".to_string());
        assert_eq!(
            cfg.selected_api_config_id,
            "legacy-openai::legacy-openai-model-default".to_string()
        );
        assert_eq!(cfg.api_configs.len(), 1);
        assert_eq!(cfg.api_configs[0].id, cfg.selected_api_config_id);
    }

    #[test]
    fn normalize_app_config_should_migrate_legacy_api_configs_when_serde_injected_default_provider() {
        let mut cfg: AppConfig = toml::from_str(
            r#"
hotkey = "Alt+·"
selectedApiConfigId = "legacy-openai"
assistantDepartmentApiConfigId = "legacy-openai"

[[apiConfigs]]
id = "legacy-openai"
name = "Legacy OpenAI"
requestFormat = "openai"
enableText = true
enableImage = false
enableAudio = false
enableTools = true
baseUrl = "https://api.openai.com/v1"
apiKey = "legacy-key"
model = "gpt-4.1"
temperature = 0.7
contextWindowTokens = 256000
maxOutputTokens = 8192
"#,
        )
        .expect("legacy toml should deserialize");

        normalize_app_config(&mut cfg);

        assert_eq!(cfg.api_providers.len(), 1);
        assert_eq!(cfg.api_providers[0].id, "legacy-openai".to_string());
        assert_eq!(cfg.api_providers[0].api_keys, vec!["legacy-key".to_string()]);
        assert_eq!(cfg.api_providers[0].models.len(), 1);
        assert_eq!(cfg.api_providers[0].models[0].model, "gpt-4.1".to_string());
        assert_eq!(
            cfg.selected_api_config_id,
            "legacy-openai::legacy-openai-model-default".to_string()
        );
    }

    #[test]
    fn consume_api_key_for_request_should_rotate_provider_keys_across_same_provider_models() {
        let provider_id = format!(
            "provider-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_millis())
                .unwrap_or(0)
        );
        let model_a = "model-a".to_string();
        let model_b = "model-b".to_string();
        let mut cfg = AppConfig {
            selected_api_config_id: api_endpoint_id(&provider_id, &model_a),
            assistant_department_api_config_id: api_endpoint_id(&provider_id, &model_a),
            api_providers: vec![ApiProviderConfig {
                id: provider_id.clone(),
                name: "OpenAI".to_string(),
                request_format: RequestFormat::OpenAI,
                enable_text: true,
                enable_image: false,
                enable_audio: false,
                enable_tools: true,
                tools: default_api_tools(),
                base_url: "https://api.openai.com/v1".to_string(),
                api_keys: vec!["key-1".to_string(), "key-2".to_string()],
                key_cursor: 0,
                cached_model_options: vec!["gpt-4.1".to_string(), "gpt-4.1-mini".to_string()],
                models: vec![
                    ApiModelConfig {
                        id: model_a.clone(),
                        model: "gpt-4.1".to_string(),
                        enable_image: false,
                        enable_tools: true,
                        temperature: 1.0,
                        custom_temperature_enabled: false,
                        context_window_tokens: 128_000,
                        max_output_tokens: 4_096,
                        custom_max_output_tokens_enabled: false,
                    },
                    ApiModelConfig {
                        id: model_b.clone(),
                        model: "gpt-4.1-mini".to_string(),
                        enable_image: false,
                        enable_tools: true,
                        temperature: 1.0,
                        custom_temperature_enabled: false,
                        context_window_tokens: 128_000,
                        max_output_tokens: 4_096,
                        custom_max_output_tokens_enabled: false,
                    },
                ],
                failure_retry_count: 0,
            }],
            api_configs: Vec::new(),
            ..AppConfig::default()
        };
        normalize_app_config(&mut cfg);

        let first = resolve_api_config(&cfg, Some(&api_endpoint_id(&provider_id, &model_a)))
            .expect("first resolve");
        let second = resolve_api_config(&cfg, Some(&api_endpoint_id(&provider_id, &model_b)))
            .expect("second resolve");
        let third = resolve_api_config(&cfg, Some(&api_endpoint_id(&provider_id, &model_a)))
            .expect("third resolve");

        assert_eq!(first.api_key, "key-1".to_string());
        assert_eq!(second.api_key, "key-1".to_string());
        assert_eq!(third.api_key, "key-1".to_string());

        let first_sent = consume_api_key_for_request(&first);
        let second_sent = consume_api_key_for_request(&second);
        let third_sent = consume_api_key_for_request(&third);

        assert_eq!(first_sent, "key-1".to_string());
        assert_eq!(second_sent, "key-2".to_string());
        assert_eq!(third_sent, "key-1".to_string());
    }
