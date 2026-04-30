/**
 * Binary file detection and handling utilities.
 * 
 * Provides functionality to:
 * - Detect binary files using isbinaryfile library
 * - Identify supported image formats
 * - Identify supported binary formats (PDF, DOCX, etc.)
 * - Route binary files to appropriate handlers
 */

import path from "path";
import { isBinaryFile } from "isbinaryfile";
import type { ToolOutput } from "@wf-agent/types";

// Supported image formats for vision models
const IMAGE_EXTENSIONS = new Set([
  ".png",
  ".jpg",
  ".jpeg",
  ".gif",
  ".bmp",
  ".svg",
  ".webp",
  ".ico",
  ".avif",
]);

// Supported binary formats for text extraction
const SUPPORTED_BINARY_FORMATS = new Set([".pdf", ".docx", ".ipynb"]);

/**
 * Result structure for binary file handling
 */
export interface BinaryFileResult {
  isBinary: boolean;
  fileType: "image" | "extractable" | "unsupported" | "text";
  extension: string;
  message?: string;
}

/**
 * Detect if a file is binary and determine its type
 * 
 * @param fullPath - Absolute path to the file
 * @param relPath - Relative path for extension detection
 * @returns Binary file detection result
 */
export async function detectBinaryFile(
  fullPath: string,
  relPath: string
): Promise<BinaryFileResult> {
  const extension = path.extname(relPath).toLowerCase();

  try {
    const isBinary = await isBinaryFile(fullPath);

    if (!isBinary) {
      return {
        isBinary: false,
        fileType: "text",
        extension,
      };
    }

    // Determine binary file type
    if (IMAGE_EXTENSIONS.has(extension)) {
      return {
        isBinary: true,
        fileType: "image",
        extension,
        message: `Image file (${extension.slice(1)})`,
      };
    }

    if (SUPPORTED_BINARY_FORMATS.has(extension)) {
      return {
        isBinary: true,
        fileType: "extractable",
        extension,
        message: `Extractable binary file (${extension.slice(1)})`,
      };
    }

    return {
      isBinary: true,
      fileType: "unsupported",
      extension,
      message: `Unsupported binary format (${extension.slice(1) || "bin"})`,
    };
  } catch (error) {
    // If detection fails, assume it's based on extension
    if (IMAGE_EXTENSIONS.has(extension)) {
      return {
        isBinary: true,
        fileType: "image",
        extension,
        message: `Image file (${extension.slice(1)})`,
      };
    }

    if (SUPPORTED_BINARY_FORMATS.has(extension)) {
      return {
        isBinary: true,
        fileType: "extractable",
        extension,
        message: `Extractable binary file (${extension.slice(1)})`,
      };
    }

    return {
      isBinary: false,
      fileType: "text",
      extension,
      message: "Could not detect file type, treating as text",
    };
  }
}

/**
 * Check if a file extension is a supported binary format for text extraction
 * 
 * @param extension - File extension (e.g., ".pdf", ".docx")
 * @returns True if the extension is supported for text extraction
 * @deprecated Special format files should use dedicated extraction scripts instead
 */
export function isSupportedBinaryFormat(extension: string): boolean {
  return SUPPORTED_BINARY_FORMATS.has(extension.toLowerCase());
}

/**
 * Get list of all supported binary formats
 * 
 * @returns Array of supported binary format extensions
 */
export function getSupportedBinaryFormats(): string[] {
  return Array.from(SUPPORTED_BINARY_FORMATS);
}

/**
 * Detect and handle binary file, returning appropriate ToolOutput
 * 
 * This is a convenience function that combines detection with basic handling logic.
 * For advanced handling (image processing, text extraction), use specialized functions.
 * 
 * @param fullPath - Absolute path to the file
 * @param relPath - Relative path for display
 * @returns ToolOutput with appropriate message or error
 */
export async function detectAndHandleBinaryFile(
  fullPath: string,
  relPath: string
): Promise<ToolOutput> {
  const detection = await detectBinaryFile(fullPath, relPath);

  if (!detection.isBinary) {
    // Not a binary file, caller should handle as text
    return {
      success: false,
      content: "",
      error: "Not a binary file",
    };
  }

  switch (detection.fileType) {
    case "image":
      return {
        success: false,
        content: "",
        error: `Image file detected (${detection.extension.slice(1)}). Use vision-capable model for image analysis.`,
      };

    case "extractable":
      return {
        success: false,
        content: "",
        error: `Binary file (${detection.extension.slice(1)}) requires text extraction. Use extract_text tool or similar.`,
      };

    case "unsupported":
      return {
        success: false,
        content: "",
        error: `Unsupported binary format (${detection.extension.slice(1) || "bin"}). Cannot read binary file contents.`,
      };

    default:
      return {
        success: false,
        content: "",
        error: `Unknown binary file type (${detection.extension})`,
      };
  }
}
