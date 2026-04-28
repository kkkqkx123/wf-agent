/**
 * Message Validator
 * Responsible for verifying message format, content type, and tool calls
 * Uses Zod for declarative validation
 */

import { z } from "zod";
import type { LLMMessage, LLMToolCall } from "@wf-agent/types";
import { MessageRole, SchemaValidationError } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import type { Result } from "@wf-agent/types";
import { all } from "@wf-agent/common-utils";

/**
 * Text to translate:
 */
const textContentSchema = z.object({
  type: z.literal("text"),
  text: z.string().min(1, "Text content cannot be empty"),
});

/**
 * Image URL Content Item Schema
 */
const imageUrlContentSchema = z.object({
  type: z.literal("image_url"),
  image_url: z.object({
    url: z.string().url("Image URL must be a valid URL"),
  }),
});

/**
 * Tool Usage Content Item Schema
 */
const toolUseContentSchema = z.object({
  type: z.literal("tool_use"),
  id: z.string().min(1, "Tool use ID cannot be empty"),
  name: z.string().min(1, "Tool use name cannot be empty"),
  input: z.record(z.string(), z.unknown()),
});

/**
 * Tool result content item schema
 */
const toolResultContentSchema = z.object({
  type: z.literal("tool_result"),
  tool_use_id: z.string().min(1, "Tool result tool_use_id cannot be empty"),
  content: z.unknown(),
});

/**
 * Content Item Schema (Union Type)
 */
const contentItemSchema = z.union([
  textContentSchema,
  imageUrlContentSchema,
  toolUseContentSchema,
  toolResultContentSchema,
]);

/**
 * Message Content Schema
 */
const messageContentSchema = z.union([
  z
    .string()
    .min(1, "Message content cannot be empty")
    .transform(val => val.trim())
    .refine(val => val.length > 0, "Message content cannot be empty"),
  z.array(contentItemSchema).min(1, "Message content array cannot be empty"),
]);

/**
 * Tool Call Function Schema
 */
const toolCallFunctionSchema = z.object({
  name: z.string().min(1, "Tool call function name cannot be empty"),
  arguments: z.string().refine(
    val => {
      try {
        JSON.parse(val);
        return true;
      } catch {
        return false;
      }
    },
    { message: "Tool call function arguments must be valid JSON" },
  ),
});

/**
 * Tool Call Schema
 */
const toolCallSchema: z.ZodType<LLMToolCall> = z.object({
  id: z.string().min(1, "Tool call ID cannot be empty"),
  type: z.literal("function"),
  function: toolCallFunctionSchema,
});

/**
 * Message Schema
 */
const messageSchema: z.ZodType<LLMMessage> = z
  .object({
    role: z.enum(["system", "user", "assistant", "tool"]),
    content: messageContentSchema,
    toolCalls: z.array(toolCallSchema).optional(),
    toolCallId: z
      .string()
      .min(1, "Tool call ID cannot be empty")
      .optional()
      .refine(val => val === undefined || val.trim().length > 0, "Tool call ID cannot be empty"),
  })
  .refine(
    data => {
      // The `tool` role must have a `toolCallId`.
      if (data.role === "tool" && !data.toolCallId) {
        return false;
      }
      return true;
    },
    { message: "Tool message must have a toolCallId", path: ["toolCallId"] },
  );

/**
 * Message Validator Class
 */
export class MessageValidator {
  /**
   * Verify the message object
   * @param message The message object
   * @returns The verification result
   */
  validateMessage(message: LLMMessage): Result<LLMMessage, SchemaValidationError[]> {
    const result = messageSchema.safeParse(message);
    if (result.success) {
      return ok(message);
    }
    return err(this.convertZodErrorToErrors(result.error));
  }

  /**
   * Verify message role
   * @param role: The role of the message
   * @returns: The verification result
   */
  validateRole(role: MessageRole): Result<MessageRole, SchemaValidationError[]> {
    if (!role) {
      return err([new SchemaValidationError("Message role is required", { field: "role" })]);
    }
    const roleSchema = z.enum(["system", "user", "assistant", "tool"]);
    const result = roleSchema.safeParse(role);
    if (result.success) {
      return ok(role);
    }
    return err([new SchemaValidationError("Invalid message role", { field: "role" })]);
  }

  /**
   * Verify message content
   * @param content: Message content
   * @param role: Message role
   * @returns: Verification result
   */
  validateContent(
    content: string | unknown[],
    role: MessageRole,
  ): Result<string | unknown[], SchemaValidationError[]> {
    const result = messageContentSchema.safeParse(content);
    if (result.success) {
      // For the `tool` role, if the content is an array, verify that all items are of the `tool_result` type.
      if (role === "tool" && Array.isArray(content)) {
        for (let i = 0; i < content.length; i++) {
          const item = content[i] as { type?: string };
          if (item.type !== "tool_result") {
            return err([
              new SchemaValidationError(
                `Tool message content item at index ${i} must have type 'tool_result'`,
                { field: `content[${i}].type` },
              ),
            ]);
          }
        }
      }
      return ok(content);
    }
    return err(this.convertZodErrorToErrors(result.error));
  }

  /**
   * Tool Call Verification
   * @param toolCalls - An array of tool calls
   * @returns - Verification results
   */
  validateToolCalls(
    toolCalls?: LLMToolCall[],
  ): Result<LLMToolCall[] | undefined, SchemaValidationError[]> {
    if (toolCalls === undefined || toolCalls === null) {
      return ok(undefined);
    }
    const result = z.array(toolCallSchema).safeParse(toolCalls);
    if (result.success) {
      return ok(toolCalls);
    }
    return err(this.convertZodErrorToErrors(result.error));
  }

  /**
   * Verify tool call ID
   * @param toolCallId: Tool call ID
   * @returns: Verification result
   */
  validateToolCallId(toolCallId?: string): Result<string | undefined, SchemaValidationError[]> {
    if (toolCallId === undefined || toolCallId === null) {
      return err([
        new SchemaValidationError("Tool message must have a toolCallId", { field: "toolCallId" }),
      ]);
    }
    const result = z
      .string()
      .min(1, "Tool call ID cannot be empty")
      .transform(val => val.trim())
      .refine(val => val.length > 0, "Tool call ID cannot be empty")
      .safeParse(toolCallId);
    if (result.success) {
      return ok(toolCallId);
    }
    return err(this.convertZodErrorToErrors(result.error));
  }

  /**
   * Batch message validation
   * @param messages Array of messages
   * @returns Validation results
   */
  validateMessages(messages: LLMMessage[]): Result<LLMMessage[], SchemaValidationError[]> {
    if (!Array.isArray(messages)) {
      return err([new SchemaValidationError("Messages must be an array", { field: "messages" })]);
    }

    const results = messages.map((message, index) => {
      const result = this.validateMessage(message);
      if (result.isErr()) {
        const errors = result.error.map(error => {
          const field = error.field ? `messages[${index}].${error.field}` : `messages[${index}]`;
          return new SchemaValidationError(error.message, { field, value: error.value });
        });
        return err(errors);
      }
      return result;
    });

    return all(results);
  }

  /**
   * Convert a Zod error to a ValidationResult
   * @param error: The Zod error
   * @returns: ValidationResult
   */
  private convertZodErrorToErrors(error: z.ZodError): SchemaValidationError[] {
    return error.issues.map(issue => {
      const field = issue.path.length > 0 ? issue.path.join(".") : undefined;
      return new SchemaValidationError(issue.message, { field });
    });
  }
}
