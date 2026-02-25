fn shell_switch_workspace_enabled_for_session(
    selected_api: &ApiConfig,
    app_state: Option<&AppState>,
    tool_session_id: &str,
) -> bool {
    if !tool_enabled(selected_api, "shell-switch-workspace") {
        return false;
    }
    let Some(state) = app_state else {
        return false;
    };
    if terminal_session_has_locked_root(state, tool_session_id) {
        return false;
    }
    true
}

fn tool_manifest_item(
    source: &str,
    name: &str,
    enabled: bool,
    attached: bool,
    reason: Option<String>,
) -> Value {
    serde_json::json!({
        "source": source,
        "name": name,
        "enabled": enabled,
        "attached": attached,
        "reason": reason
    })
}

async fn assemble_runtime_tools(
    selected_api: &ApiConfig,
    app_state: Option<&AppState>,
    tool_session_id: &str,
) -> Result<RuntimeToolAssembly, String> {
    let has_fetch = tool_enabled(selected_api, "fetch");
    let has_bing = tool_enabled(selected_api, "bing-search");
    let has_memory = tool_enabled(selected_api, "memory-save");
    let has_desktop_screenshot = tool_enabled(selected_api, "desktop-screenshot");
    let has_desktop_wait = tool_enabled(selected_api, "desktop-wait");
    let has_refresh_mcp_skills = tool_enabled(selected_api, "refresh-mcp-skills");
    let has_shell_switch_workspace =
        shell_switch_workspace_enabled_for_session(selected_api, app_state, tool_session_id);
    let has_shell_exec = tool_enabled(selected_api, "shell-exec");

    let mut tools: Vec<Box<dyn ToolDyn>> = Vec::new();
    let mut tool_manifest = Vec::<Value>::new();

    tool_manifest.push(tool_manifest_item(
        "builtin",
        "fetch",
        has_fetch,
        has_fetch,
        if has_fetch {
            None
        } else {
            Some("disabled in api tools config".to_string())
        },
    ));
    if has_fetch {
        tools.push(Box::new(BuiltinFetchTool));
    }

    tool_manifest.push(tool_manifest_item(
        "builtin",
        "bing-search",
        has_bing,
        has_bing,
        if has_bing {
            None
        } else {
            Some("disabled in api tools config".to_string())
        },
    ));
    if has_bing {
        tools.push(Box::new(BuiltinBingSearchTool));
    }

    if has_memory {
        let state = app_state
            .ok_or_else(|| "memory_save requires app state".to_string())?
            .clone();
        tools.push(Box::new(BuiltinMemorySaveTool {
            app_state: state,
        }));
        tool_manifest.push(tool_manifest_item(
            "builtin",
            "memory-save",
            true,
            true,
            None,
        ));
    } else {
        tool_manifest.push(tool_manifest_item(
            "builtin",
            "memory-save",
            false,
            false,
            Some("disabled in api tools config".to_string()),
        ));
    }

    let mut mcp_screenshot_client: Option<ScreenshotMcpClient> = None;
    if has_desktop_screenshot {
        match try_attach_desktop_screenshot_mcp_tool(&mut tools).await {
            Ok(client) => {
                mcp_screenshot_client = Some(client);
                tool_manifest.push(tool_manifest_item(
                    "builtin_mcp",
                    "desktop-screenshot",
                    true,
                    true,
                    None,
                ));
            }
            Err(err) => {
                eprintln!("[MCP] desktop-screenshot degraded to disabled: {err}");
                tool_manifest.push(tool_manifest_item(
                    "builtin_mcp",
                    "desktop-screenshot",
                    true,
                    false,
                    Some(format!("MCP attach failed: {err}")),
                ));
            }
        }
    } else {
        tool_manifest.push(tool_manifest_item(
            "builtin_mcp",
            "desktop-screenshot",
            false,
            false,
            Some("disabled in api tools config".to_string()),
        ));
    }

    match attach_enabled_mcp_tools_for_runtime(&mut tools, app_state).await {
        Ok(names) => {
            if names.is_empty() {
                tool_manifest.push(tool_manifest_item(
                    "mcp_runtime",
                    "(none)",
                    true,
                    false,
                    Some("no enabled MCP tools attached".to_string()),
                ));
            } else {
                for name in names {
                    tool_manifest.push(tool_manifest_item(
                        "mcp_runtime",
                        &name,
                        true,
                        true,
                        None,
                    ));
                }
            }
        }
        Err(err) => {
            tool_manifest.push(tool_manifest_item(
                "mcp_runtime",
                "(attach)",
                true,
                false,
                Some(err.clone()),
            ));
            eprintln!("[MCP] attach runtime tools skipped: {err}");
        }
    }

    if has_desktop_wait {
        tools.push(Box::new(BuiltinDesktopWaitTool));
        tool_manifest.push(tool_manifest_item(
            "builtin",
            "desktop-wait",
            true,
            true,
            None,
        ));
    } else {
        tool_manifest.push(tool_manifest_item(
            "builtin",
            "desktop-wait",
            false,
            false,
            Some("disabled in api tools config".to_string()),
        ));
    }

    if has_refresh_mcp_skills {
        let state = app_state
            .ok_or_else(|| "refresh_mcp_and_skills requires app state".to_string())?
            .clone();
        tools.push(Box::new(BuiltinRefreshMcpAndSkillsTool { app_state: state }));
        tool_manifest.push(tool_manifest_item(
            "builtin",
            "refresh-mcp-skills",
            true,
            true,
            None,
        ));
    } else {
        tool_manifest.push(tool_manifest_item(
            "builtin",
            "refresh-mcp-skills",
            false,
            false,
            Some("disabled in api tools config".to_string()),
        ));
    }

    if has_shell_switch_workspace {
        let state = app_state
            .ok_or_else(|| "shell_switch_workspace requires app state".to_string())?
            .clone();
        tools.push(Box::new(BuiltinShellSwitchWorkspaceTool {
            app_state: state,
            session_id: tool_session_id.to_string(),
        }));
        tool_manifest.push(tool_manifest_item(
            "builtin",
            "shell-switch-workspace",
            true,
            true,
            None,
        ));
    } else {
        tool_manifest.push(tool_manifest_item(
            "builtin",
            "shell-switch-workspace",
            false,
            false,
            Some("disabled in api tools config or locked workspace".to_string()),
        ));
    }

    if has_shell_exec {
        let state = app_state
            .ok_or_else(|| "shell_exec requires app state".to_string())?
            .clone();
        tools.push(Box::new(BuiltinTerminalExecTool {
            app_state: state,
            session_id: tool_session_id.to_string(),
        }));
        tool_manifest.push(tool_manifest_item(
            "builtin",
            "shell-exec",
            true,
            true,
            None,
        ));
    } else {
        tool_manifest.push(tool_manifest_item(
            "builtin",
            "shell-exec",
            false,
            false,
            Some("disabled in api tools config".to_string()),
        ));
    }

    Ok(RuntimeToolAssembly {
        tools,
        tool_manifest,
        _mcp_screenshot_client: mcp_screenshot_client,
    })
}

async fn try_attach_desktop_screenshot_mcp_tool(
    tools: &mut Vec<Box<dyn ToolDyn>>,
) -> Result<ScreenshotMcpClient, String> {
    let exe = std::env::current_exe()
        .map_err(|err| format!("Resolve current executable for MCP screenshot failed: {err}"))?;
    let mut cmd = tokio::process::Command::new(exe);
    cmd.arg(MCP_SCREENSHOT_SERVER_FLAG);
    let transport = rmcp::transport::TokioChildProcess::new(cmd)
        .map_err(|err| format!("Start MCP screenshot child process failed: {err}"))?;

    let client = ().serve(transport).await.map_err(|err| {
        format!("Connect to MCP screenshot server failed: {err}")
    })?;
    let sink = client.peer().clone();
    let defs = client
        .list_all_tools()
        .await
        .map_err(|err| format!("List MCP screenshot tools failed: {err}"))?;

    let mut attached = false;
    for def in defs {
        if def.name.as_ref() != MCP_SCREENSHOT_TOOL_NAME {
            continue;
        }
        tools.push(Box::new(rig::tool::rmcp::McpTool::from_mcp_server(
            def,
            sink.clone(),
        )));
        attached = true;
        break;
    }

    if !attached {
        return Err("MCP screenshot server did not expose desktop_screenshot tool".to_string());
    }
    Ok(client)
}
