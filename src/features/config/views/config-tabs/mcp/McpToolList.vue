<template>
  <ToolListCard
    :title="t('config.mcpToolList.toolList')"
    :items="toolItems"
    :disabled="disabled"
    :refreshable="true"
    :refresh-label="t('config.mcp.refresh')"
    :no-description-text="t('config.mcpToolList.noDescription')"
    :elapsed-label="t('config.mcpToolList.recentElapsed')"
    :elapsed-ms="elapsedMs"
    @toggle-item="onToggleItem"
    @refresh="emit('refreshTools')"
  >
    <template #item-extra="{ item }">
      <div v-if="toolParameterSummary(item.id).length" class="mt-1 flex flex-wrap gap-1">
        <span
          v-for="paramText in toolParameterSummary(item.id)"
          :key="`${item.id}-param-${paramText}`"
          class="text-[10px] px-1.5 py-0.5 rounded bg-base-200 border border-base-300/70 opacity-80"
        >
          {{ paramText }}
        </span>
      </div>
      <div v-if="toolParameterExamples(item.id).length" class="mt-1 grid gap-1">
        <pre
          v-for="example in toolParameterExamples(item.id)"
          :key="`${item.id}-example-${example}`"
          class="text-[10px] leading-4 px-2 py-1 rounded bg-base-200 border border-base-300/70 opacity-90 whitespace-pre-wrap overflow-x-auto"
        >{{ example }}</pre>
      </div>
    </template>
  </ToolListCard>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import type { McpToolDescriptor } from "../../../../../types/app";
import ToolListCard, { type ToolListItem } from "../../../components/ToolListCard.vue";

const { t } = useI18n();

const props = defineProps<{
  tools: McpToolDescriptor[];
  elapsedMs: number;
  disabled?: boolean;
}>();

const emit = defineEmits<{
  (e: "toggleTool", payload: { toolName: string; enabled: boolean }): void;
  (e: "refreshTools"): void;
}>();

const toolItems = computed<ToolListItem[]>(() =>
  props.tools.map((tool) => ({
    id: tool.toolName,
    name: tool.toolName,
    description: tool.description,
    enabled: tool.enabled,
    statusClass: tool.enabled ? "bg-success" : "bg-base-content/30",
    statusTitle: tool.enabled ? t("config.mcp.toolEnabled") : t("config.mcp.toolDisabled"),
  })),
);

function onToggleItem(payload: { id: string; enabled: boolean }) {
  emit("toggleTool", {
    toolName: payload.id,
    enabled: payload.enabled,
  });
}

function toolById(id: string): McpToolDescriptor | undefined {
  return props.tools.find((tool) => tool.toolName === id);
}

function toolParameterSummary(id: string): string[] {
  const parameters = toolById(id)?.parameters;
  if (!parameters || typeof parameters !== "object") return [];
  const root = parameters as Record<string, unknown>;
  const propertiesRaw = root.properties;
  const requiredRaw = Array.isArray(root.required) ? root.required : [];
  if (!propertiesRaw || typeof propertiesRaw !== "object") return [];
  const properties = propertiesRaw as Record<string, unknown>;
  return Object.entries(properties).map(([name, schema]) => {
    const shape = schema && typeof schema === "object" ? (schema as Record<string, unknown>) : {};
    const typeValue = Array.isArray(shape.type)
      ? shape.type.map(String).join(" | ")
      : String(shape.type || "any");
    const required = requiredRaw.includes(name) ? "*" : "";
    const enumValues = Array.isArray(shape.enum) ? ` [${shape.enum.map(String).join(", ")}]` : "";
    const minText = shape.minimum !== undefined ? ` >= ${shape.minimum}` : "";
    const maxText = shape.maximum !== undefined ? ` <= ${shape.maximum}` : "";
    const desc = String(shape.description || "").trim();
    const rangeText = `${enumValues}${minText}${maxText}`.trim();
    const base = `${required}${name}: ${typeValue}${rangeText ? ` ${rangeText}` : ""}`;
    return desc ? `${base} (${desc})` : base;
  });
}

function formatSchemaExample(value: unknown): string {
  if (typeof value === "string") {
    return value.trim();
  }
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

function toolParameterExamples(id: string): string[] {
  const parameters = toolById(id)?.parameters;
  if (!parameters || typeof parameters !== "object") return [];
  const root = parameters as Record<string, unknown>;
  const propertiesRaw = root.properties;
  if (!propertiesRaw || typeof propertiesRaw !== "object") return [];
  const properties = propertiesRaw as Record<string, unknown>;
  const examples: string[] = [];
  for (const [name, schema] of Object.entries(properties)) {
    const shape = schema && typeof schema === "object" ? (schema as Record<string, unknown>) : {};
    if (shape.example !== undefined && shape.example !== null) {
      const singleExample = formatSchemaExample(shape.example);
      if (singleExample) {
        examples.push(`${name} 示例:\n${singleExample}`);
      }
    }
    const exampleList = Array.isArray(shape.examples) ? shape.examples : [];
    for (const rawExample of exampleList) {
      if (rawExample === undefined || rawExample === null) continue;
      const text = formatSchemaExample(rawExample);
      if (text) {
        examples.push(`${name} 示例:\n${text}`);
      }
    }
  }
  return Array.from(new Set(examples));
}
</script>
