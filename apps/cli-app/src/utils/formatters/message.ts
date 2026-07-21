/**
 * Message formatters
 */

import { getFormatter } from "../formatter.js";
import { formatWith, truncate, emptyMsg } from "./utils.js";
import type { Message } from "@wf-agent/types";

// Helper function to extract text content from MessageContent
function getMessageText(content: Message): string {
  if (typeof content.content === "string") {
    return content.content;
  }
  // For array content, try to extract text from first text element
  if (Array.isArray(content.content)) {
    const textElement = content.content.find(item => item.type === "text");
    return textElement?.text || "";
  }
  return "";
}

export function formatMessage(message: Message, options?: { verbose?: boolean }): string {
  return formatWith(message, options, () => {
    const role = message.role || "N/A";
    const content = getMessageText(message);
    const preview = truncate(content, 50);
    return `${role}: ${preview}`;
  });
}

export function formatMessageList(messages: Message[], options?: { table?: boolean }): string {
  if (messages.length === 0) {
    return emptyMsg("messages");
  }

  if (options?.table) {
    const formatter = getFormatter();
    const headers = ["Role", "Content preview", "Time"];
    const rows = messages.map(m => {
      const content = getMessageText(m);
      const preview = truncate(content, 30);
      return [m.role || "N/A", preview, String(m.timestamp || "N/A")];
    });
    return formatter.table(headers, rows);
  }

  return messages.map(m => formatMessage(m)).join("\n");
}