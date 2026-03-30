<template>
  <aside class="flex h-full w-88 shrink-0 flex-col border-r border-base-300 bg-base-200">
    <div class="flex-1 min-h-0 space-y-2 overflow-y-auto py-2">
      <button
        v-for="item in items"
        :key="item.conversationId"
        type="button"
        class="mx-2 block w-[calc(100%-1rem)] rounded-box bg-base-200 text-left transition-colors hover:bg-base-100"
        :class="[
          item.conversationId === activeConversationId ? 'bg-primary/10 hover:bg-primary/10' : '',
          item.runtimeState === 'organizing_context' ? 'cursor-not-allowed opacity-60' : '',
        ]"
        :disabled="item.runtimeState === 'organizing_context'"
        :title="item.runtimeState === 'organizing_context' ? t('chat.organizingContextDisabled') : (item.workspaceLabel || t('chat.defaultWorkspace'))"
        @click="emit('select', item.conversationId)"
      >
        <div class="flex items-center gap-3 p-3">
          <div class="shrink-0">
            <div class="avatar">
              <div class="flex h-10 w-10 items-center justify-center rounded-full bg-neutral text-neutral-content">
                <img
                  v-if="sideListLastSpeakerAvatarUrl(item)"
                  :src="sideListLastSpeakerAvatarUrl(item)"
                  :alt="sideListLastSpeakerLabel(item)"
                  class="w-10 h-10 rounded-full object-cover"
                />
                <span v-else class="text-sm font-bold">{{ sideListLastSpeakerInitial(item) }}</span>
              </div>
            </div>
          </div>

          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-2">
              <div class="truncate text-sm font-medium">
                {{ item.title || t("chat.untitledConversation") }}
              </div>
              <span v-if="item.isMainConversation" class="badge badge-primary badge-xs shrink-0">
                {{ t("chat.mainConversation") }}
              </span>
              <span v-if="item.conversationId === activeConversationId" class="badge badge-outline badge-xs shrink-0">
                {{ t("chat.currentConversation") }}
              </span>
            </div>

            <div class="mt-1 flex items-center gap-2 text-xs">
              <span class="font-medium">{{ item.workspaceLabel || t("chat.defaultWorkspace") }}</span>
              <span class="text-base-content/70">{{ formatConversationTime(item.updatedAt) }}</span>
              <span class="font-medium">{{ t("chat.messageCount", { count: item.messageCount }) }}</span>
            </div>
          </div>

          <span v-if="item.runtimeState" class="badge badge-ghost badge-xs shrink-0">
            {{ runtimeStateText(item.runtimeState) }}
          </span>
        </div>

        <div class="space-y-1 px-3 pb-3">
          <div
            v-for="preview in normalizedPreviewMessages(item).slice(0, 2)"
            :key="preview.messageId"
            class="flex items-start gap-2 text-xs"
          >
            <span class="shrink-0 font-medium">{{ speakerLabel(preview) }}:</span>
            <span class="truncate opacity-80">{{ previewText(preview) }}</span>
          </div>
          <div v-if="normalizedPreviewMessages(item).length === 0" class="px-2 text-xs opacity-60">
            {{ t("chat.conversationNoPreview") }}
          </div>
        </div>
      </button>
    </div>
  </aside>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import type { ChatConversationOverviewItem, ConversationPreviewMessage } from "../../../types/app";

const props = defineProps<{
  items: ChatConversationOverviewItem[];
  activeConversationId: string;
  userAlias: string;
  userAvatarUrl: string;
  personaNameMap: Record<string, string>;
  personaAvatarUrlMap: Record<string, string>;
}>();

const emit = defineEmits<{
  (e: "select", conversationId: string): void;
}>();

const { t } = useI18n();

const conversationPreviewCache = computed(() => new Map(
  props.items.map((item) => [String(item.conversationId || "").trim(), Array.isArray(item.previewMessages) ? item.previewMessages : []]),
));

function normalizedPreviewMessages(item: ChatConversationOverviewItem): ConversationPreviewMessage[] {
  return conversationPreviewCache.value.get(String(item.conversationId || "").trim()) || [];
}

function runtimeStateText(runtimeState?: ChatConversationOverviewItem["runtimeState"]): string {
  if (runtimeState === "assistant_streaming") return t("chat.runtimeStreaming");
  if (runtimeState === "organizing_context") return t("chat.runtimeOrganizing");
  return t("chat.runtimeIdle");
}

function speakerLabel(preview: ConversationPreviewMessage): string {
  if (preview.role === "tool") return t("archives.roleTool");
  const speakerId = String(preview.speakerAgentId || "").trim();
  if (!speakerId || speakerId === "user-persona") {
    return props.userAlias || t("archives.roleUser");
  }
  return props.personaNameMap?.[speakerId] || speakerId;
}

function previewText(preview: ConversationPreviewMessage): string {
  const text = String(preview.textPreview || "").trim();
  if (text) return text;
  if (preview.hasPdf) return t("chat.previewPdf");
  if (preview.hasImage) return t("chat.previewImage");
  if (preview.hasAudio) return t("chat.previewAudio");
  if (preview.hasAttachment) return t("chat.previewAttachment");
  return t("chat.conversationNoPreview");
}

function formatConversationTime(value?: string): string {
  if (!value) return "-";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString(undefined, {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function sideListLastSpeakerInitial(item: ChatConversationOverviewItem): string {
  const previews = normalizedPreviewMessages(item);
  if (previews.length === 0) return "?";
  return speakerLabel(previews[previews.length - 1]).charAt(0).toUpperCase();
}

function sideListLastSpeakerLabel(item: ChatConversationOverviewItem): string {
  const previews = normalizedPreviewMessages(item);
  if (previews.length === 0) return "";
  return speakerLabel(previews[previews.length - 1]);
}

function sideListLastSpeakerAvatarUrl(item: ChatConversationOverviewItem): string {
  const previews = normalizedPreviewMessages(item);
  if (previews.length === 0) return "";
  const speakerId = String(previews[previews.length - 1].speakerAgentId || "").trim();
  if (!speakerId || speakerId === "user-persona") {
    return props.userAvatarUrl || "";
  }
  return props.personaAvatarUrlMap?.[speakerId] || "";
}
</script>
