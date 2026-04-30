/**
 * Image processing and validation utilities.
 * 
 * Provides functionality for:
 * - Validating image files for processing
 * - Processing images for vision models
 * - Tracking image memory usage
 */

import fs from "fs/promises";
import path from "path";

/**
 * Default maximum image file size in MB
 */
export const DEFAULT_MAX_IMAGE_FILE_SIZE_MB = 10;

/**
 * Default maximum total image size in MB (for multiple images)
 */
export const DEFAULT_MAX_TOTAL_IMAGE_SIZE_MB = 50;

/**
 * Result of image validation
 */
export interface ImageValidationResult {
  isValid: boolean;
  notice?: string;
  error?: string;
}

/**
 * Result of image processing
 */
export interface ImageProcessResult {
  dataUrl: string;
  sizeInMB: number;
  notice: string;
}

/**
 * Tracker for cumulative image memory usage
 */
export class ImageMemoryTracker {
  private totalMemoryUsed: number = 0;

  getTotalMemoryUsed(): number {
    return this.totalMemoryUsed;
  }

  addMemoryUsage(sizeInMB: number): void {
    this.totalMemoryUsed += sizeInMB;
  }

  reset(): void {
    this.totalMemoryUsed = 0;
  }
}

/**
 * Validate an image file for processing.
 * 
 * @param fullPath - Absolute path to the image file
 * @param supportsImages - Whether the model supports images
 * @param maxImageFileSize - Maximum single image file size in MB
 * @param maxTotalImageSize - Maximum total image size in MB
 * @param currentMemoryUsed - Current memory used by other images
 * @returns Validation result
 */
export async function validateImageForProcessing(
  fullPath: string,
  supportsImages: boolean,
  maxImageFileSize: number = DEFAULT_MAX_IMAGE_FILE_SIZE_MB,
  maxTotalImageSize: number = DEFAULT_MAX_TOTAL_IMAGE_SIZE_MB,
  currentMemoryUsed: number = 0
): Promise<ImageValidationResult> {
  if (!supportsImages) {
    return {
      isValid: false,
      notice: "Model does not support image processing",
    };
  }

  try {
    const stats = await fs.stat(fullPath);
    const fileSizeInMB = stats.size / (1024 * 1024);

    // Check individual file size
    if (fileSizeInMB > maxImageFileSize) {
      return {
        isValid: false,
        notice: `Image file too large (${fileSizeInMB.toFixed(2)}MB > ${maxImageFileSize}MB limit)`,
      };
    }

    // Check total memory usage
    if (currentMemoryUsed + fileSizeInMB > maxTotalImageSize) {
      return {
        isValid: false,
        notice: `Total image memory would exceed limit (${(currentMemoryUsed + fileSizeInMB).toFixed(2)}MB > ${maxTotalImageSize}MB)`,
      };
    }

    return {
      isValid: true,
    };
  } catch (error) {
    return {
      isValid: false,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

/**
 * Process an image file and convert to base64 data URL.
 * 
 * @param fullPath - Absolute path to the image file
 * @returns Processing result with data URL
 */
export async function processImageFile(fullPath: string): Promise<ImageProcessResult> {
  try {
    const buffer = await fs.readFile(fullPath);
    const extension = path.extname(fullPath).toLowerCase();
    
    // Determine MIME type based on extension
    const mimeTypes: Record<string, string> = {
      ".png": "image/png",
      ".jpg": "image/jpeg",
      ".jpeg": "image/jpeg",
      ".gif": "image/gif",
      ".bmp": "image/bmp",
      ".svg": "image/svg+xml",
      ".webp": "image/webp",
      ".ico": "image/x-icon",
      ".avif": "image/avif",
    };

    const mimeType = mimeTypes[extension] || "application/octet-stream";
    const base64 = buffer.toString("base64");
    const dataUrl = `data:${mimeType};base64,${base64}`;
    
    const sizeInMB = buffer.length / (1024 * 1024);

    return {
      dataUrl,
      sizeInMB,
      notice: `Image processed successfully (${sizeInMB.toFixed(2)}MB)`,
    };
  } catch (error) {
    throw new Error(
      `Failed to process image: ${error instanceof Error ? error.message : String(error)}`
    );
  }
}
