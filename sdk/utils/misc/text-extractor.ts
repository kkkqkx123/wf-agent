/**
 * Text extraction utilities for binary formats.
 * 
 * Provides functionality for:
 * - Extracting text from PDF files
 * - Extracting text from DOCX files
 * - Adding line numbers to extracted content
 */

import { formatLineNumbers } from "../tool-utils.js";

/**
 * Result of text extraction
 */
export interface TextExtractionResult {
  success: boolean;
  content: string;
  lineCount: number;
  error?: string;
}

/**
 * Add line numbers to text content.
 * 
 * @param content - Text content to format
 * @returns Content with line numbers added
 */
export function addLineNumbers(content: string): string {
  const lines = content.split("\n");
  return formatLineNumbers(lines, 1);
}

/**
 * Extract text from a binary file (PDF, DOCX, etc.).
 * 
 * Note: This is a placeholder implementation. In production, you would integrate
 * with libraries like pdf-parse for PDFs or mammoth for DOCX files.
 * 
 * @param fullPath - Absolute path to the binary file
 * @param extension - File extension (e.g., ".pdf", ".docx")
 * @returns Extraction result with text content
 */
export async function extractTextFromBinary(
  fullPath: string,
  extension: string
): Promise<TextExtractionResult> {
  try {
    // Placeholder: In production, use appropriate libraries
    // For PDF: import pdfParse from 'pdf-parse'
    // For DOCX: import mammoth from 'mammoth'
    
    switch (extension.toLowerCase()) {
      case ".pdf":
        // TODO: Implement PDF text extraction
        // const dataBuffer = await fs.readFile(fullPath);
        // const data = await pdfParse(dataBuffer);
        // return { success: true, content: data.text, lineCount: data.text.split('\n').length };
        return {
          success: false,
          content: "",
          lineCount: 0,
          error: "PDF text extraction not yet implemented. Install pdf-parse library.",
        };

      case ".docx":
        // TODO: Implement DOCX text extraction
        // const result = await mammoth.extractRawText({ path: fullPath });
        // return { success: true, content: result.value, lineCount: result.value.split('\n').length };
        return {
          success: false,
          content: "",
          lineCount: 0,
          error: "DOCX text extraction not yet implemented. Install mammoth library.",
        };

      case ".ipynb":
        // Jupyter notebook - can be read as JSON
        const fs = await import("fs/promises");
        const notebookContent = await fs.readFile(fullPath, "utf-8");
        const notebook = JSON.parse(notebookContent);
        
        // Extract code cells
        const codeCells = notebook.cells
          ?.filter((cell: any) => cell.cell_type === "code")
          ?.map((cell: any) => cell.source?.join("\n") || "")
          ?.filter((source: string) => source.length > 0) || [];
        
        const extractedText = codeCells.join("\n\n");
        return {
          success: true,
          content: extractedText,
          lineCount: extractedText.split("\n").length,
        };

      default:
        return {
          success: false,
          content: "",
          lineCount: 0,
          error: `Unsupported binary format: ${extension}`,
        };
    }
  } catch (error) {
    return {
      success: false,
      content: "",
      lineCount: 0,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}
