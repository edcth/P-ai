<template>
  <dialog class="modal" :class="{ 'modal-open': open }">
    <div class="modal-box max-w-4xl h-[70vh] flex flex-col">
      <div class="flex items-center justify-between gap-2">
        <h3 class="font-semibold text-base">运行日志（内存）</h3>
        <div class="join">
          <button class="btn btn-sm join-item" :disabled="logs.length === 0" @click="copyVisibleLogs">复制</button>
          <button class="btn btn-sm join-item" :disabled="loading" @click="$emit('refresh')">刷新</button>
          <button class="btn btn-sm join-item" :disabled="loading || logs.length === 0" @click="$emit('clear')">清空</button>
          <button class="btn btn-sm btn-primary join-item" @click="$emit('close')">关闭</button>
        </div>
      </div>
      <div class="text-xs opacity-70 mt-1">仅保留内存日志，容量上限 10MB，进程退出即清空。</div>
      <div v-if="copyStatus" class="mt-1 text-xs opacity-70">{{ copyStatus }}</div>
      <div class="mt-2 flex flex-wrap items-center gap-2 text-xs">
        <label class="flex items-center gap-1">
          <span class="opacity-70">级别</span>
          <select v-model="selectedLevel" class="select select-bordered select-xs w-28">
            <option value="all">全部</option>
            <option v-for="level in levelOptions" :key="level" :value="level">
              {{ level.toUpperCase() }}
            </option>
          </select>
        </label>
        <label class="flex items-center gap-1">
          <span class="opacity-70">模块</span>
          <select v-model="selectedModule" class="select select-bordered select-xs w-48">
            <option value="all">全部</option>
            <option v-for="moduleName in moduleOptions" :key="moduleName" :value="moduleName">
              {{ moduleName }}
            </option>
          </select>
        </label>
        <span class="opacity-70">显示 {{ filteredLogs.length }} / {{ logs.length }}</span>
      </div>
      <div v-if="errorText" class="text-error text-sm mt-2">{{ errorText }}</div>
      <pre class="mt-3 flex-1 overflow-auto rounded-box border border-base-300 bg-base-100 p-3 font-mono text-xs leading-5 whitespace-pre-wrap break-words"><code>{{ runtimeLogCode }}</code></pre>
    </div>
    <form method="dialog" class="modal-backdrop">
      <button @click.prevent="$emit('close')">close</button>
    </form>
  </dialog>
</template>

<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { RuntimeLogEntry } from "../../../types/app";

const props = defineProps<{
  open: boolean;
  logs: RuntimeLogEntry[];
  loading: boolean;
  errorText: string;
}>();

defineEmits<{
  (e: "close"): void;
  (e: "refresh"): void;
  (e: "clear"): void;
}>();

const levelOptions = ["info", "warn", "error", "debug", "trace"] as const;
const selectedLevel = ref<"all" | (typeof levelOptions)[number]>("info");
const selectedModule = ref("all");
const copyStatus = ref("");

const moduleOptions = computed(() => {
  const moduleSet = new Set<string>();
  for (const item of props.logs) {
    const moduleName = extractModuleName(item.message);
    if (!moduleName) continue;
    moduleSet.add(moduleName);
  }
  return Array.from(moduleSet).sort((a, b) => a.localeCompare(b, "zh-CN"));
});

const filteredLogs = computed(() =>
  props.logs.filter((item) => {
    if (selectedLevel.value !== "all" && item.level !== selectedLevel.value) {
      return false;
    }
    if (selectedModule.value !== "all" && extractModuleName(item.message) !== selectedModule.value) {
      return false;
    }
    return true;
  }),
);

watch(
  () => [selectedLevel.value, selectedModule.value, filteredLogs.value.length],
  () => {
    copyStatus.value = "";
  },
);

const runtimeLogCode = computed(() => {
  if (props.loading) return "正在加载运行日志...";
  if (filteredLogs.value.length === 0) return "暂无日志";
  return filteredLogs.value.map(formatLogLine).join("\n");
});

function formatLogLine(item: RuntimeLogEntry): string {
  const segments = [
    `[${formatLogTime(item.createdAt)}]`,
    item.level.toUpperCase(),
    item.message,
  ];
  if (item.repeat > 1) {
    segments.push(`x${item.repeat}`);
  }
  return segments.join(" ");
}

async function copyVisibleLogs() {
  const text = filteredLogs.value.map(formatLogLine).join("\n");
  if (!text) {
    copyStatus.value = "当前没有可复制的日志";
    return;
  }
  try {
    await navigator.clipboard.writeText(text);
    copyStatus.value = `已复制 ${filteredLogs.value.length} 条日志`;
  } catch {
    copyStatus.value = "复制失败，请检查系统剪贴板权限";
  }
}

function extractModuleName(message: string): string | null {
  const text = String(message || "").trim();
  if (!text.startsWith("[")) {
    return null;
  }
  const matched = text.match(/^\[([^\]]+)\]/);
  if (!matched || !matched[1]) {
    return null;
  }
  return matched[1].trim() || null;
}

function formatLogTime(value: string): string {
  const raw = String(value || "").trim();
  if (!raw) return "";
  const date = new Date(raw);
  if (Number.isNaN(date.getTime())) {
    return raw.replace("T", " ").replace(/(\.\d+)?Z?$/, "");
  }
  const yyyy = date.getFullYear();
  const mm = String(date.getMonth() + 1).padStart(2, "0");
  const dd = String(date.getDate()).padStart(2, "0");
  const hh = String(date.getHours()).padStart(2, "0");
  const mi = String(date.getMinutes()).padStart(2, "0");
  const ss = String(date.getSeconds()).padStart(2, "0");
  return `${yyyy}-${mm}-${dd} ${hh}:${mi}:${ss}`;
}
</script>
