import { ref } from "vue";
import type { ChatMessageBlock } from "../../../types/app";

export function useChatMessageActions() {
  const playingAudioId = ref("");
  let activeAudio: HTMLAudioElement | null = null;

  function splitThinkText(raw: string): { visible: string; inline: string } {
    const input = raw || "";
    const openTag = "<think>";
    const closeTag = "</think>";
    const blocks: string[] = [];
    let visible = "";
    let cursor = 0;

    while (cursor < input.length) {
      const openIdx = input.indexOf(openTag, cursor);
      if (openIdx < 0) {
        visible += input.slice(cursor);
        break;
      }

      visible += input.slice(cursor, openIdx);
      const afterOpen = openIdx + openTag.length;
      const closeIdx = input.indexOf(closeTag, afterOpen);
      if (closeIdx < 0) {
        const tail = input.slice(afterOpen).trim();
        if (tail) blocks.push(tail);
        cursor = input.length;
        break;
      }

      const inner = input.slice(afterOpen, closeIdx).trim();
      if (inner) blocks.push(inner);
      cursor = closeIdx + closeTag.length;
    }

    return {
      visible: visible.trim(),
      inline: blocks.join("\n\n"),
    };
  }

  async function copyMessage(block: ChatMessageBlock) {
    const copyText = splitThinkText(block.text).visible || block.text || "";
    if (!copyText) return;
    try {
      await navigator.clipboard.writeText(copyText);
    } catch {
      // Ignore clipboard failures to avoid interrupting chat flow.
    }
  }

  function buildAudioDataUrl(audio: { mime: string; bytesBase64: string }): string {
    return `data:${audio.mime};base64,${audio.bytesBase64}`;
  }

  function stopAudioPlayback() {
    if (activeAudio) {
      activeAudio.pause();
      activeAudio.currentTime = 0;
      activeAudio = null;
    }
    playingAudioId.value = "";
  }

  function toggleAudioPlayback(id: string, audio: { mime: string; bytesBase64: string }) {
    if (playingAudioId.value === id && activeAudio) {
      stopAudioPlayback();
      return;
    }
    stopAudioPlayback();
    const player = new Audio(buildAudioDataUrl(audio));
    activeAudio = player;
    playingAudioId.value = id;
    player.onended = () => {
      if (activeAudio === player) {
        activeAudio = null;
        playingAudioId.value = "";
      }
    };
    void player.play().catch(() => {
      if (activeAudio === player) {
        activeAudio = null;
        playingAudioId.value = "";
      }
    });
  }

  return {
    playingAudioId,
    copyMessage,
    stopAudioPlayback,
    toggleAudioPlayback,
  };
}
