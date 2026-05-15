/**
 * SSE (Server-Sent Events) Utilities
 * Provides common SSE parsing and streaming functionality
 */

/**
 * Parse SSE data from a text chunk
 * Extracts data from SSE format: "data: <content>"
 */
export function parseSSELine(line: string): unknown | null {
  if (!line.startsWith("data: ")) {
    return null;
  }

  const data = line.slice(6); // Remove "data: " prefix

  // Skip special markers
  if (data === "[DONE]" || data.trim() === "") {
    return null;
  }

  try {
    return JSON.parse(data);
  } catch {
    // If not JSON, return as string
    return data;
  }
}

/**
 * Parse SSE data from multiple lines
 * Handles buffer accumulation and line splitting
 */
export function parseSSELines(lines: string[]): unknown[] {
  const results: unknown[] = [];

  for (const line of lines) {
    const data = parseSSELine(line);
    if (data !== null) {
      results.push(data);
    }
  }

  return results;
}

/**
 * Stream SSE data from a ReadableStream
 * Returns an async iterable of parsed SSE messages
 */
export async function* streamSSE(
  stream: ReadableStream<Uint8Array>
): AsyncIterable<unknown> {
  const reader = stream.getReader();
  const decoder = new TextDecoder();
  let buffer = "";

  try {
    while (true) {
      const { done, value } = await reader.read();

      if (done) {
        // Process remaining buffer
        if (buffer.trim()) {
          const data = parseSSELine(buffer.trim());
          if (data !== null) {
            yield data;
          }
        }
        break;
      }

      // Add new data to buffer
      buffer += decoder.decode(value, { stream: true });

      // Split by lines (handle both \n and \r\n)
      const lines = buffer.split(/\r?\n/);
      buffer = lines.pop() || ""; // Keep incomplete last line

      // Process each complete line
      for (const line of lines) {
        const data = parseSSELine(line);
        if (data !== null) {
          yield data;
        }
      }
    }
  } finally {
    reader.releaseLock();
  }
}

/**
 * Read SSE stream and call handler for each message
 * Alternative to async iterable for callback-based usage
 */
export async function readSSEStream(
  stream: ReadableStream<Uint8Array>,
  handler: (data: unknown) => void
): Promise<void> {
  for await (const data of streamSSE(stream)) {
    handler(data);
  }
}
