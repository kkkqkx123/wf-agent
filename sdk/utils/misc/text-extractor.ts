/**
 * Text extraction utilities for binary formats.
 * 
 * Provides functionality for:
 * - Extracting text from PDF files (using pdf-parse)
 * - Extracting text from DOCX files (using mammoth)
 * - Extracting text from XLSX files (using exceljs)
 * - Extracting text from Jupyter notebooks (.ipynb)
 * - Adding line numbers to extracted content
 * 
 * Note: PDF, DOCX, and XLSX extraction require optional dependencies.
 * Use TextExtractorManager for lazy loading with clear error messages.
 */

import fs from "fs/promises";
import { formatLineNumbers } from "../tool-utils.js";
import { TextExtractorManager } from "./text-extractor-manager.js";

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
 * Extract text from a PDF file using pdf-parse library.
 * 
 * @param fullPath - Absolute path to the PDF file
 * @returns Extraction result with text content
 */
async function extractTextFromPDF(fullPath: string): Promise<TextExtractionResult> {
  try {
    // Use manager for lazy loading with proper error handling
    const pdfParse = await TextExtractorManager.getPdfParse();
    
    // Read file as buffer
    const dataBuffer = await fs.readFile(fullPath);
    
    // Parse PDF - pdf-parse v2.x exports a default function
    const result = await pdfParse.default ? await pdfParse.default(dataBuffer) : await pdfParse(dataBuffer);
    const content = result.text;
    
    return {
      success: true,
      content,
      lineCount: content.split("\n").length,
    };
  } catch (error) {
    return {
      success: false,
      content: "",
      lineCount: 0,
      error: `PDF extraction failed: ${error instanceof Error ? error.message : String(error)}`,
    };
  }
}

/**
 * Extract text from a DOCX file using mammoth library.
 * 
 * @param fullPath - Absolute path to the DOCX file
 * @returns Extraction result with text content
 */
async function extractTextFromDOCX(fullPath: string): Promise<TextExtractionResult> {
  try {
    // Use manager for lazy loading with proper error handling
    const mammoth = await TextExtractorManager.getMammoth();
    
    const result = await mammoth.extractRawText({ path: fullPath });
    const content = result.value;
    
    return {
      success: true,
      content,
      lineCount: content.split("\n").length,
    };
  } catch (error) {
    return {
      success: false,
      content: "",
      lineCount: 0,
      error: `DOCX extraction failed: ${error instanceof Error ? error.message : String(error)}`,
    };
  }
}

/**
 * Extract text from an XLSX file using exceljs library.
 * 
 * @param fullPath - Absolute path to the XLSX file
 * @returns Extraction result with text content
 */
async function extractTextFromXLSX(fullPath: string): Promise<TextExtractionResult> {
  try {
    // Use manager for lazy loading with proper error handling
    const ExcelJS = await TextExtractorManager.getExcelJS();
    
    const workbook = new ExcelJS.Workbook();
    await workbook.xlsx.readFile(fullPath);
    
    let excelText = "";
    const ROW_LIMIT = 50000;
    
    workbook.eachSheet((worksheet: any) => {
      if (worksheet.state === "hidden" || worksheet.state === "veryHidden") {
        return;
      }
      
      excelText += `--- Sheet: ${worksheet.name} ---\n`;
      
      worksheet.eachRow({ includeEmpty: false }, (row: any, rowNumber: number) => {
        if (rowNumber > ROW_LIMIT) {
          excelText += `[... truncated at row ${rowNumber} ...]\n`;
          return false;
        }
        
        const rowTexts: string[] = [];
        let hasContent = false;
        
        row.eachCell({ includeEmpty: true }, (cell: any) => {
          const cellText = formatCellValue(cell);
          if (cellText.trim()) {
            hasContent = true;
          }
          rowTexts.push(cellText);
        });
        
        if (hasContent) {
          excelText += rowTexts.join("\t") + "\n";
        }
        
        return true;
      });
      
      excelText += "\n";
    });
    
    const content = excelText.trim();
    return {
      success: true,
      content,
      lineCount: content.split("\n").length,
    };
  } catch (error) {
    return {
      success: false,
      content: "",
      lineCount: 0,
      error: `XLSX extraction failed: ${error instanceof Error ? error.message : String(error)}`,
    };
  }
}

/**
 * Format a single Excel cell value to string.
 */
function formatCellValue(cell: any): string {
  const value = cell.value;
  if (value === null || value === undefined) {
    return "";
  }
  
  // Handle error values (#DIV/0!, #N/A, etc.)
  if (typeof value === "object" && "error" in value) {
    return `[Error: ${value.error}]`;
  }
  
  // Handle dates - ExcelJS can parse them as Date objects
  if (value instanceof Date) {
    return value.toISOString().split("T")[0] || "";
  }
  
  // Handle rich text
  if (typeof value === "object" && "richText" in value) {
    return value.richText.map((rt: any) => rt.text).join("");
  }
  
  // Handle hyperlinks
  if (typeof value === "object" && "text" in value && "hyperlink" in value) {
    return `${value.text} (${value.hyperlink})`;
  }
  
  // Handle formulas - get the calculated result
  if (typeof value === "object" && "formula" in value) {
    if ("result" in value && value.result !== undefined && value.result !== null) {
      return value.result.toString();
    } else {
      return `[Formula: ${value.formula}]`;
    }
  }
  
  return value.toString();
}

/**
 * Extract text from a binary file (PDF, DOCX, XLSX, IPYNB, etc.).
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
    switch (extension.toLowerCase()) {
      case ".pdf":
        return await extractTextFromPDF(fullPath);
      
      case ".docx":
        return await extractTextFromDOCX(fullPath);
      
      case ".xlsx":
        return await extractTextFromXLSX(fullPath);
      
      case ".ipynb":
        // Jupyter notebook - can be read as JSON
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
