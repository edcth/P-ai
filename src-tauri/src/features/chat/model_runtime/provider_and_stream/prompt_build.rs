fn build_tool_loop_prompt(
    prepared: &PreparedPrompt,
) -> Result<(RigMessage, Vec<RigMessage>), String> {
    let mut prompt_blocks: Vec<UserContent> = Vec::new();
    if !prepared.latest_user_text.trim().is_empty() {
        prompt_blocks.push(UserContent::text(prepared.latest_user_text.clone()));
    }
    if !prepared.latest_user_time_text.trim().is_empty() {
        prompt_blocks.push(UserContent::text(prepared.latest_user_time_text.clone()));
    }
    if !prepared.latest_user_system_text.trim().is_empty() {
        prompt_blocks.push(UserContent::text(prepared.latest_user_system_text.clone()));
    }
    let current_prompt_content = OneOrMany::many(prompt_blocks)
        .map_err(|_| "Request payload is empty. Provide text, image, or audio.".to_string())?;
    let current_prompt: RigMessage = RigMessage::User {
        content: current_prompt_content,
    };
    let chat_history = prepared_history_to_rig_messages(prepared)?;
    Ok((current_prompt, chat_history))
}
