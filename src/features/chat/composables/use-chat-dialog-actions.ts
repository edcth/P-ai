import { invokeTauri } from "../../../services/tauri-api";
import type { Ref } from "vue";

type UseChatDialogActionsOptions = {
  activeChatApiConfigId: Ref<string>;
  assistantDepartmentAgentId: Ref<string>;
  openPromptPreviewDialog: (apiConfigId: string, agentId: string) => Promise<void>;
  openSystemPromptPreviewDialog: (apiConfigId: string, agentId: string) => Promise<void>;
};

export function useChatDialogActions(options: UseChatDialogActionsOptions) {
  async function openCurrentHistory() {
    await invokeTauri("show_archives_window");
  }

  async function openPromptPreview() {
    if (!options.activeChatApiConfigId.value || !options.assistantDepartmentAgentId.value) return;
    await options.openPromptPreviewDialog(options.activeChatApiConfigId.value, options.assistantDepartmentAgentId.value);
  }

  async function openSystemPromptPreview() {
    if (!options.activeChatApiConfigId.value || !options.assistantDepartmentAgentId.value) return;
    await options.openSystemPromptPreviewDialog(options.activeChatApiConfigId.value, options.assistantDepartmentAgentId.value);
  }

  return {
    openCurrentHistory,
    openPromptPreview,
    openSystemPromptPreview,
  };
}


