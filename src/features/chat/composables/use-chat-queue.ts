import { ref, onMounted, onUnmounted } from "vue";
import { invokeTauri } from "../../../services/tauri-api";

export type ChatQueueEvent = {
  id: string;
  source: "user" | "task" | "delegate" | "system";
  createdAt: string;
  messagePreview: string;
  conversationId: string;
};

export type MainSessionState = "idle" | "assistant_streaming" | "organizing_context";

export function useChatQueue() {
  const queueEvents = ref<ChatQueueEvent[]>([]);
  const sessionState = ref<MainSessionState>("idle");
  const polling = ref(false);
  let pollTimer: ReturnType<typeof setInterval> | null = null;

  async function refreshQueue() {
    try {
      const events = await invokeTauri<ChatQueueEvent[]>("get_chat_queue_snapshot");
      queueEvents.value = events || [];
    } catch (error) {
      console.error("[CHAT-QUEUE] Failed to refresh queue:", error);
      queueEvents.value = [];
    }
  }

  async function refreshSessionState() {
    try {
      const state = await invokeTauri<MainSessionState>("get_main_session_state_snapshot");
      sessionState.value = state || "idle";
    } catch (error) {
      console.error("[CHAT-QUEUE] Failed to refresh session state:", error);
    }
  }

  async function removeFromQueue(eventId: string): Promise<boolean> {
    try {
      const removed = await invokeTauri<boolean>("remove_chat_queue_event", { eventId });
      if (removed) {
        await refreshQueue();
      }
      return removed;
    } catch (error) {
      console.error("[CHAT-QUEUE] Failed to remove event:", error);
      return false;
    }
  }

  function startPolling(intervalMs = 1000) {
    if (polling.value) return;
    polling.value = true;

    // 立即刷新一次
    refreshQueue();
    refreshSessionState();

    pollTimer = setInterval(() => {
      refreshQueue();
      refreshSessionState();
    }, intervalMs);
  }

  function stopPolling() {
    if (pollTimer) {
      clearInterval(pollTimer);
      pollTimer = null;
    }
    polling.value = false;
  }

  onMounted(() => {
    startPolling();
  });

  onUnmounted(() => {
    stopPolling();
  });

  return {
    queueEvents,
    sessionState,
    polling,
    refreshQueue,
    refreshSessionState,
    removeFromQueue,
    startPolling,
    stopPolling,
  };
}
