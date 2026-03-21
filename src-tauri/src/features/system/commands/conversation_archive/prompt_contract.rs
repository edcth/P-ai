fn build_archive_summary_only_instruction(agent: &AgentProfile, user_alias: &str) -> String {
    format!(
        "你要做归档总结。仅输出纯文本 summary，不要 JSON，不要 markdown 代码块。\n\
         你是 {assistant_name}，用户称谓是 {user_name}。\n\
         \n\
         强制要求：\n\
         1) 只描述“已经发生过的事实”，禁止描述“接下来准备做什么/将要做什么”。\n\
         2) 按时间顺序回顾：曾经做了什么、确认了什么、留下了什么未完成事项。\n\
         3) 必须使用以下 8 个固定段落标题，顺序不可变：\n\
            - Current Progress\n\
            - Current State\n\
            - User Decisions\n\
            - Open Issue (Root Cause)\n\
            - What Changed\n\
            - What Remains\n\
            - Constraints / Preferences\n\
            - Quick References\n\
         4) 每个段落都必须有内容；没有信息时写“无”。\n\
         5) What Remains 只能写当前仍未完成的事实，不写执行承诺和计划句。\n\
         6) 保留可追溯锚点：关键对象、关键时间点、关键数字或约束条件。",
        assistant_name = agent.name,
        user_name = user_alias
    )
}

fn build_archive_summary_only_latest_user_text(
    instruction: &str,
    all_compaction_context: &str,
    used_memories: &str,
) -> String {
    format!(
        "<归档提示>\n{}\n</归档提示>\n\n<本会话全部上下文整理信息>\n{}\n</本会话全部上下文整理信息>\n\n<本次会话使用过的记忆>\n{}\n</本次会话使用过的记忆>",
        instruction, all_compaction_context, used_memories
    )
}

