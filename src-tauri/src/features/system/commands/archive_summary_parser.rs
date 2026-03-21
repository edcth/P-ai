#[derive(Debug, Clone, Deserialize)]
struct ArchiveMemoryDraft {
    #[serde(default, alias = "memoryType")]
    memory_type: String,
    #[serde(default, alias = "content")]
    judgment: String,
    #[serde(default)]
    reasoning: String,
    #[serde(default, alias = "keywords")]
    tags: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArchiveMergeGroupDraft {
    #[serde(default)]
    source_ids: Vec<String>,
    target: ArchiveMemoryDraft,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MemoryCurationDraft {
    #[serde(default)]
    useful_memory_ids: Vec<String>,
    #[serde(default, alias = "memories")]
    new_memories: Vec<ArchiveMemoryDraft>,
    #[serde(default)]
    merge_groups: Vec<ArchiveMergeGroupDraft>,
}

fn parse_memory_curation_draft_from_value(value: serde_json::Value) -> Option<MemoryCurationDraft> {
    let obj = value.as_object()?;
    let has_any_useful_key = obj.contains_key("usefulMemoryIds")
        || obj.contains_key("useful_memory_ids")
        || obj.contains_key("newMemories")
        || obj.contains_key("new_memories")
        || obj.contains_key("memories")
        || obj.contains_key("mergeGroups")
        || obj.contains_key("merge_groups");
    if !has_any_useful_key {
        return None;
    }

    let useful_memory_ids = obj
        .get("usefulMemoryIds")
        .or_else(|| obj.get("useful_memory_ids"))
        .and_then(serde_json::Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(serde_json::Value::as_str)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let new_memories = obj
        .get("newMemories")
        .or_else(|| obj.get("new_memories"))
        .or_else(|| obj.get("memories"))
        .and_then(serde_json::Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|item| serde_json::from_value::<ArchiveMemoryDraft>(item.clone()).ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let merge_groups = obj
        .get("mergeGroups")
        .or_else(|| obj.get("merge_groups"))
        .and_then(serde_json::Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    serde_json::from_value::<ArchiveMergeGroupDraft>(item.clone()).ok()
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Some(MemoryCurationDraft {
        useful_memory_ids,
        new_memories,
        merge_groups,
    })
}

fn parse_memory_curation_draft(raw: &str) -> Option<MemoryCurationDraft> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(parsed) = serde_json::from_str::<MemoryCurationDraft>(trimmed) {
        return Some(parsed);
    }
    if let Ok(parsed_value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(parsed) = parse_memory_curation_draft_from_value(parsed_value) {
            return Some(parsed);
        }
    }
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    if end <= start {
        return None;
    }
    let snippet = &trimmed[start..=end];
    if let Ok(parsed) = serde_json::from_str::<MemoryCurationDraft>(snippet) {
        return Some(parsed);
    }
    let parsed_value = serde_json::from_str::<serde_json::Value>(snippet).ok()?;
    parse_memory_curation_draft_from_value(parsed_value)
}
