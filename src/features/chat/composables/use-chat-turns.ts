import { computed, type ComputedRef, type Ref, type ShallowRef } from "vue";
import type { ApiConfigItem, ChatMessage, ChatTurn } from "../../../types/app";
import {
  estimateConversationTokens,
  extractMessageAudios,
  extractMessageImages,
  parseAssistantStoredText,
  removeBinaryPlaceholders,
  renderMessage,
} from "../../../utils/chat-message";

type UseChatTurnsOptions = {
  allMessages: ShallowRef<ChatMessage[]>;
  visibleTurnCount: Ref<number>;
  activeChatApiConfig: ComputedRef<ApiConfigItem | null>;
  perfDebug: boolean;
  perfNow: () => number;
};

export function useChatTurns(options: UseChatTurnsOptions) {
  function summarizeAssistantToolHistory(
    toolHistory: ChatMessage["toolCall"],
  ): { count: number; lastToolName: string } {
    if (!Array.isArray(toolHistory) || toolHistory.length === 0) {
      return { count: 0, lastToolName: "" };
    }
    let count = 0;
    let lastToolName = "";
    for (const event of toolHistory) {
      if (!event || event.role !== "assistant" || !Array.isArray(event.tool_calls)) continue;
      for (const call of event.tool_calls) {
        const name = String(call?.function?.name || "").trim();
        if (!name) continue;
        count += 1;
        lastToolName = name;
      }
    }
    return { count, lastToolName };
  }

  const allTurns = computed<ChatTurn[]>(() => {
    const startedAt = options.perfNow();
    const msgs = options.allMessages.value;
    const turns: ChatTurn[] = [];
    for (let i = 0; i < msgs.length; i++) {
      const msg = msgs[i];
      if (msg.role === "user") {
        const userText = removeBinaryPlaceholders(renderMessage(msg));
        const userImages = extractMessageImages(msg);
        const userAudios = extractMessageAudios(msg);
        let assistantText = "";
        let assistantReasoningStandard = "";
        let assistantReasoningInline = "";
        let assistantToolCallCount = 0;
        let assistantLastToolName = "";
        if (i + 1 < msgs.length && msgs[i + 1].role === "assistant") {
          const assistantMsg = msgs[i + 1];
          const parsed = parseAssistantStoredText(renderMessage(assistantMsg));
          const providerMeta = assistantMsg.providerMeta || {};
          const toolSummary = summarizeAssistantToolHistory(assistantMsg.toolCall);
          assistantText = parsed.assistantText;
          assistantReasoningStandard = parsed.reasoningStandard || String(providerMeta.reasoningStandard || "");
          assistantReasoningInline = parsed.reasoningInline || String(providerMeta.reasoningInline || "");
          assistantToolCallCount = toolSummary.count;
          assistantLastToolName = toolSummary.lastToolName;
          i++;
        }
        if (
          userText
          || userImages.length > 0
          || userAudios.length > 0
          || assistantText.trim()
          || assistantReasoningStandard.trim()
          || assistantReasoningInline.trim()
        ) {
          turns.push({
            id: msg.id,
            userText,
            userImages,
            userAudios,
            assistantText,
            assistantReasoningStandard,
            assistantReasoningInline,
            assistantToolCallCount,
            assistantLastToolName,
          });
        }
      }
    }
    if (options.perfDebug) {
      const cost = Math.round((options.perfNow() - startedAt) * 10) / 10;
      console.log(`[PERF] buildAllTurns messages=${msgs.length} turns=${turns.length} cost=${cost}ms`);
    }
    return turns;
  });

  const visibleTurns = computed(() =>
    allTurns.value.slice(Math.max(0, allTurns.value.length - options.visibleTurnCount.value))
  );

  const hasMoreTurns = computed(() => options.visibleTurnCount.value < allTurns.value.length);

  const chatContextUsageRatio = computed(() => {
    const api = options.activeChatApiConfig.value;
    if (!api) return 0;
    const maxTokens = Math.max(16000, Math.min(200000, Number(api.contextWindowTokens ?? 128000)));
    const used = estimateConversationTokens(options.allMessages.value);
    return used / Math.max(1, maxTokens);
  });

  const chatUsagePercent = computed(() => Math.min(100, Math.max(0, Math.round(chatContextUsageRatio.value * 100))));

  return {
    allTurns,
    visibleTurns,
    hasMoreTurns,
    chatContextUsageRatio,
    chatUsagePercent,
  };
}
