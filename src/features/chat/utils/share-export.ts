import { invokeTauri } from "../../../services/tauri-api";
import type { ChatMessageBlock } from "../../../types/app";

export type ShareRenderableEntry = {
  id: string;
  align: "left" | "right";
  displayName: string;
  createdAtText: string;
  text: string;
  reasoningText: string;
  toolCalls: Array<{ name: string; argsText: string; status?: string }>;
  images: Array<{ src: string; alt: string }>;
  attachmentNames: string[];
  audioCount: number;
  remoteContactLabel: string;
};

type PrepareShareEntriesOptions = {
  blocks: ChatMessageBlock[];
  userAlias: string;
  personaNameMap: Record<string, string>;
  trigger?: string;
};

type BuildShareDocumentOptions = {
  title: string;
  subtitle?: string;
  entries: ShareRenderableEntry[];
};

const SHARE_EXPORT_WIDTH = 900;

export async function prepareShareEntries(
  options: PrepareShareEntriesOptions,
): Promise<ShareRenderableEntry[]> {
  const startedAt = performance.now();
  const entries = await Promise.all(
    options.blocks.map(async (block, index): Promise<ShareRenderableEntry> => ({
      id: String(block.id || block.sourceMessageId || `share-${index}`).trim() || `share-${index}`,
      align: isOwnShareBlock(block) ? "right" : "left",
      displayName: shareDisplayName(block, options.userAlias, options.personaNameMap),
      createdAtText: formatShareTime(block.createdAt),
      text: String(block.text || "").trim(),
      reasoningText: normalizeReasoningText(block),
      toolCalls: Array.isArray(block.toolCalls)
        ? block.toolCalls.map((call) => ({
          name: String(call?.name || "").trim(),
          argsText: String(call?.argsText || "").trim(),
          status: String(call?.status || "").trim() || undefined,
        })).filter((call) => !!call.name || !!call.argsText)
        : [],
      images: await Promise.allSettled(
        (Array.isArray(block.images) ? block.images : []).map(async (image, imageIndex) => {
          try {
            const src = await resolveShareImageSrc(image);
            if (!src) return null;
            return {
              src,
              alt: `image-${index + 1}-${imageIndex + 1}`,
            };
          } catch (error) {
            console.warn("[分享导出] 图片资源解析失败，已跳过", {
              fn: "prepareShareEntries",
              trigger: options.trigger || "unknown",
              blockId: block.id || block.sourceMessageId || `share-${index}`,
              imageIndex,
              error: String(error),
            });
            return null;
          }
        }),
      ).then((results) => results
        .map((result) => (result.status === "fulfilled" ? result.value : null))
        .filter((item): item is { src: string; alt: string } => !!item?.src)),
      attachmentNames: Array.isArray(block.attachmentFiles)
        ? block.attachmentFiles
          .map((item) => String(item?.fileName || "").trim())
          .filter((item) => !!item)
        : [],
      audioCount: Array.isArray(block.audios) ? block.audios.length : 0,
      remoteContactLabel: block.remoteImOrigin
        ? String(block.remoteImOrigin.senderName || block.remoteImOrigin.remoteContactName || "").trim()
        : "",
    })),
  );
  console.info("[分享导出] 消息条目准备完成", {
    task: "prepareShareEntries",
    trigger: options.trigger || "unknown",
    inputCount: options.blocks.length,
    outputCount: entries.length,
    durationMs: Math.round(performance.now() - startedAt),
  });
  return entries;
}

export function buildShareHtmlDocument(options: BuildShareDocumentOptions): string {
  return `<!doctype html>
<html lang="zh-CN">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>${escapeHtml(options.title)}</title>
    <style>${shareDocumentCss()}</style>
  </head>
  <body>
    ${buildShareBodyHtml(options)}
  </body>
</html>`;
}

export async function renderShareDocumentToPngDataUrl(
  options: BuildShareDocumentOptions,
): Promise<string> {
  const host = document.createElement("div");
  host.setAttribute(
    "style",
    `position:fixed;left:-100000px;top:0;width:${SHARE_EXPORT_WIDTH}px;opacity:0;pointer-events:none;z-index:-1;`,
  );
  host.innerHTML = `<style>${shareDocumentCss()}</style>${buildShareBodyHtml(options)}`;
  document.body.appendChild(host);

  try {
    let page: HTMLElement | null = null;
    let width = SHARE_EXPORT_WIDTH;
    let height = 200;
    let pixelRatio = 1;
    let svgUrl = "";
    try {
      await waitForImages(host);
      page = host.querySelector(".pai-share-page") as HTMLElement | null;
      if (!page) {
        throw new Error("分享预览渲染失败：未找到渲染页面节点");
      }
      width = Math.max(SHARE_EXPORT_WIDTH, Math.ceil(page.scrollWidth));
      height = Math.max(200, Math.ceil(page.scrollHeight));
      const svg = buildShareSvg({
        width,
        height,
        bodyHtml: buildShareBodyHtml(options),
        cssText: shareDocumentCss(),
      });
      const svgBlob = new Blob([svg], { type: "image/svg+xml;charset=utf-8" });
      svgUrl = URL.createObjectURL(svgBlob);
      const image = await loadImage(svgUrl);
      pixelRatio = Math.min(Math.max(window.devicePixelRatio || 1, 1), 2);
      const canvas = document.createElement("canvas");
      canvas.width = Math.max(1, Math.round(width * pixelRatio));
      canvas.height = Math.max(1, Math.round(height * pixelRatio));
      const context = canvas.getContext("2d");
      if (!context) {
        throw new Error("创建分享画布失败：无法获取 2D 绘图上下文");
      }
      context.scale(pixelRatio, pixelRatio);
      context.fillStyle = "#f7f4ef";
      context.fillRect(0, 0, width, height);
      context.drawImage(image, 0, 0, width, height);
      return canvas.toDataURL("image/png");
    } catch (error) {
      console.warn("[分享导出] 图片渲染失败", {
        fn: "renderShareDocumentToPngDataUrl",
        pageFound: !!page,
        width,
        height,
        pixelRatio,
        title: options.title,
        subtitle: options.subtitle || "",
        entryCount: options.entries.length,
        error: String(error),
      });
      throw error instanceof Error
        ? new Error(`${error.message} (fn=renderShareDocumentToPngDataUrl width=${width} height=${height} pixelRatio=${pixelRatio} entries=${options.entries.length})`)
        : new Error(`分享图片导出失败 (fn=renderShareDocumentToPngDataUrl width=${width} height=${height} pixelRatio=${pixelRatio} entries=${options.entries.length} error=${String(error)})`);
    } finally {
      if (svgUrl) {
        URL.revokeObjectURL(svgUrl);
      }
    }
  } finally {
    host.remove();
  }
}

export function buildShareExportFileName(kind: "html" | "png"): string {
  const stamp = new Date().toISOString().replace(/[:.]/g, "-");
  return kind === "html"
    ? `p-ai-share-${stamp}.html`
    : `p-ai-share-${stamp}.png`;
}

function buildShareBodyHtml(options: BuildShareDocumentOptions): string {
  const subtitle = String(options.subtitle || "").trim();
  return `<main class="pai-share-page">
    <header class="pai-share-header">
      <div class="pai-share-title">${escapeHtml(options.title)}</div>
      ${subtitle ? `<div class="pai-share-subtitle">${escapeHtml(subtitle)}</div>` : ""}
    </header>
    <section class="pai-share-list">
      ${options.entries.map(renderShareEntryHtml).join("")}
    </section>
  </main>`;
}

function renderShareEntryHtml(entry: ShareRenderableEntry): string {
  const textHtml = entry.text
    ? `<div class="pai-share-text">${renderTextHtml(entry.text)}</div>`
    : "";
  const reasoningHtml = entry.reasoningText
    ? `<div class="pai-share-extra pai-share-reasoning"><div class="pai-share-extra-label">思考</div><div class="pai-share-extra-body">${renderTextHtml(entry.reasoningText)}</div></div>`
    : "";
  const toolsHtml = entry.toolCalls.length > 0
    ? `<div class="pai-share-extra pai-share-tools"><div class="pai-share-extra-label">工具</div><ul class="pai-share-tool-list">${entry.toolCalls.map((toolCall) => `<li class="pai-share-tool-item"><div class="pai-share-tool-name">${escapeHtml(toolCall.name || "tool")}${toolCall.status ? `<span class="pai-share-tool-status">${escapeHtml(toolCall.status)}</span>` : ""}</div>${toolCall.argsText ? `<pre class="pai-share-tool-args">${escapeHtml(toolCall.argsText)}</pre>` : ""}</li>`).join("")}</ul></div>`
    : "";
  const imageHtml = entry.images.length > 0
    ? `<div class="pai-share-images">${entry.images.map((image) => `<img class="pai-share-image" src="${escapeHtmlAttribute(image.src)}" alt="${escapeHtmlAttribute(image.alt)}" />`).join("")}</div>`
    : "";
  const filesHtml = entry.attachmentNames.length > 0 || entry.audioCount > 0
    ? `<div class="pai-share-meta-row">${entry.attachmentNames.map((name) => `<span class="pai-share-chip">附件 · ${escapeHtml(name)}</span>`).join("")}${entry.audioCount > 0 ? `<span class="pai-share-chip">语音 × ${entry.audioCount}</span>` : ""}</div>`
    : "";
  const remoteHtml = entry.remoteContactLabel
    ? `<div class="pai-share-remote-label">${escapeHtml(entry.remoteContactLabel)}</div>`
    : "";
  return `<article class="pai-share-entry pai-share-entry-${entry.align}">
    <div class="pai-share-entry-header">
      <div class="pai-share-display-name">${escapeHtml(entry.displayName)}</div>
      <div class="pai-share-time">${escapeHtml(entry.createdAtText)}</div>
    </div>
    ${remoteHtml}
    <div class="pai-share-bubble pai-share-bubble-${entry.align}">
      ${textHtml}
      ${reasoningHtml}
      ${toolsHtml}
      ${imageHtml}
      ${filesHtml}
    </div>
  </article>`;
}

function shareDocumentCss(): string {
  return `
    * { box-sizing: border-box; }
    html, body {
      margin: 0;
      padding: 0;
      background: #f7f4ef;
      color: #1f2937;
      font-family: "Segoe UI", "Microsoft YaHei", sans-serif;
    }
    .pai-share-page {
      width: ${SHARE_EXPORT_WIDTH}px;
      margin: 0 auto;
      padding: 28px 24px 32px;
      background: linear-gradient(180deg, #fff8f1 0%, #f7f4ef 100%);
    }
    .pai-share-header {
      margin-bottom: 18px;
      padding: 18px 20px;
      border-radius: 18px;
      background: #ffffff;
      border: 1px solid rgba(31, 41, 55, 0.08);
      box-shadow: 0 12px 30px rgba(15, 23, 42, 0.06);
    }
    .pai-share-title {
      font-size: 22px;
      font-weight: 700;
      line-height: 1.4;
    }
    .pai-share-subtitle {
      margin-top: 6px;
      color: rgba(31, 41, 55, 0.7);
      font-size: 13px;
      line-height: 1.6;
    }
    .pai-share-list {
      display: flex;
      flex-direction: column;
      gap: 14px;
    }
    .pai-share-entry {
      display: flex;
      flex-direction: column;
      gap: 6px;
    }
    .pai-share-entry-right {
      align-items: flex-end;
    }
    .pai-share-entry-left {
      align-items: flex-start;
    }
    .pai-share-entry-header {
      width: 100%;
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      padding: 0 6px;
      font-size: 12px;
      color: rgba(31, 41, 55, 0.65);
    }
    .pai-share-display-name {
      font-weight: 600;
      color: #111827;
    }
    .pai-share-time {
      white-space: nowrap;
    }
    .pai-share-remote-label {
      padding: 0 6px;
      font-size: 11px;
      color: rgba(31, 41, 55, 0.55);
    }
    .pai-share-bubble {
      max-width: 86%;
      padding: 14px 16px;
      border-radius: 20px;
      border: 1px solid rgba(31, 41, 55, 0.08);
      box-shadow: 0 10px 24px rgba(15, 23, 42, 0.04);
      white-space: normal;
    }
    .pai-share-bubble-left {
      background: #ffffff;
    }
    .pai-share-bubble-right {
      background: #ffe9ef;
    }
    .pai-share-text,
    .pai-share-extra-body,
    .pai-share-tool-args {
      white-space: pre-wrap;
      overflow-wrap: anywhere;
      word-break: break-word;
      line-height: 1.7;
      font-size: 14px;
    }
    .pai-share-extra {
      margin-top: 12px;
      padding-top: 12px;
      border-top: 1px dashed rgba(31, 41, 55, 0.14);
    }
    .pai-share-extra-label {
      margin-bottom: 6px;
      font-size: 11px;
      font-weight: 700;
      color: rgba(31, 41, 55, 0.58);
      letter-spacing: 0.04em;
    }
    .pai-share-tool-list {
      list-style: none;
      padding: 0;
      margin: 0;
      display: flex;
      flex-direction: column;
      gap: 8px;
    }
    .pai-share-tool-name {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      font-size: 12px;
      font-weight: 700;
      color: #111827;
    }
    .pai-share-tool-status {
      font-size: 10px;
      padding: 1px 6px;
      border-radius: 999px;
      background: rgba(59, 130, 246, 0.12);
      color: #1d4ed8;
    }
    .pai-share-tool-args {
      margin: 4px 0 0;
      padding: 10px 12px;
      border-radius: 12px;
      background: rgba(31, 41, 55, 0.05);
      color: rgba(17, 24, 39, 0.86);
    }
    .pai-share-images {
      margin-top: 12px;
      display: flex;
      flex-wrap: wrap;
      gap: 8px;
    }
    .pai-share-image {
      max-width: 220px;
      max-height: 220px;
      border-radius: 14px;
      display: block;
      background: rgba(31, 41, 55, 0.05);
      object-fit: contain;
    }
    .pai-share-meta-row {
      display: flex;
      flex-wrap: wrap;
      gap: 6px;
      margin-top: 12px;
    }
    .pai-share-chip {
      display: inline-flex;
      align-items: center;
      gap: 4px;
      border-radius: 999px;
      padding: 4px 10px;
      background: rgba(31, 41, 55, 0.07);
      font-size: 11px;
      color: rgba(31, 41, 55, 0.76);
    }
  `;
}

function buildShareSvg(options: {
  width: number;
  height: number;
  bodyHtml: string;
  cssText: string;
}): string {
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${options.width}" height="${options.height}" viewBox="0 0 ${options.width} ${options.height}">
  <foreignObject width="100%" height="100%">
    <div xmlns="http://www.w3.org/1999/xhtml">
      <style>${options.cssText}</style>
      ${options.bodyHtml}
    </div>
  </foreignObject>
</svg>`;
}

async function resolveShareImageSrc(image: {
  mime: string;
  bytesBase64?: string;
  mediaRef?: string;
}): Promise<string> {
  const mime = String(image.mime || "").trim() || "image/png";
  const bytesBase64 = String(image.bytesBase64 || "").trim();
  if (bytesBase64) {
    return `data:${mime};base64,${bytesBase64}`;
  }
  const mediaRef = String(image.mediaRef || "").trim();
  if (!mediaRef) return "";
  try {
    const result = await invokeTauri<{ dataUrl: string }>("read_chat_image_data_url", {
      input: {
        mediaRef,
        mime,
      },
    });
    return String(result?.dataUrl || "").trim();
  } catch (error) {
    console.warn("[分享导出] 读取图片数据失败，已跳过", {
      fn: "resolveShareImageSrc",
      mediaRef,
      mime,
      error: String(error),
    });
    return "";
  }
}

function shareDisplayName(
  block: ChatMessageBlock,
  userAlias: string,
  personaNameMap: Record<string, string>,
): string {
  if (block.remoteImOrigin) {
    return String(
      block.remoteImOrigin.senderName
      || block.remoteImOrigin.remoteContactName
      || "联系人",
    ).trim();
  }
  const speakerAgentId = String(block.speakerAgentId || "").trim();
  if (!speakerAgentId || speakerAgentId === "user-persona" || block.role === "user") {
    return String(userAlias || "用户").trim() || "用户";
  }
  const mapped = String(personaNameMap[speakerAgentId] || "").trim();
  if (mapped) return mapped;
  return speakerAgentId || String(block.role || "assistant").trim() || "assistant";
}

function isOwnShareBlock(block: ChatMessageBlock): boolean {
  if (block.remoteImOrigin) return false;
  if (block.role === "user") return true;
  const speakerAgentId = String(block.speakerAgentId || "").trim();
  return speakerAgentId === "user-persona";
}

function normalizeReasoningText(block: ChatMessageBlock): string {
  const standard = String(block.reasoningStandard || "").trim();
  if (standard) return standard;
  return String(block.reasoningInline || "").trim();
}

function formatShareTime(input?: string): string {
  const raw = String(input || "").trim();
  if (!raw) return "";
  const date = new Date(raw);
  if (Number.isNaN(date.getTime())) return raw;
  return date.toLocaleString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function renderTextHtml(text: string): string {
  return escapeHtml(text).replace(/\n/g, "<br/>");
}

function escapeHtml(value: string): string {
  return String(value)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/\"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function escapeHtmlAttribute(value: string): string {
  return escapeHtml(value).replace(/\n/g, " ");
}

async function waitForImages(root: HTMLElement): Promise<void> {
  const images = Array.from(root.querySelectorAll("img"));
  if (images.length === 0) return;
  await Promise.all(
    images.map((image) => new Promise<void>((resolve, reject) => {
      if (image.complete && image.naturalWidth > 0) {
        resolve();
        return;
      }
      image.onload = () => resolve();
      image.onerror = () => reject(new Error(`图片加载失败: ${image.currentSrc || image.src || "unknown"}`));
    })),
  );
}

async function loadImage(url: string): Promise<HTMLImageElement> {
  return await new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error("分享图片渲染失败"));
    image.src = url;
  });
}
