/**
 * TextExtractorManager - Manages the lifecycle of text extraction libraries.
 *
 * Provides functionality:
 * - Lazy loading of optional dependencies (pdf-parse, mammoth, exceljs)
 * - Singleton pattern ensuring workflow-execution safety
 * - Test-friendly reset capability
 * - Clear error handling with installation suggestions
 * - Type-safe access to library instances
 *
 * Usage example:
 *   // PDF extraction
 *   const pdfLib = await TextExtractorManager.getPdfParse();
 *   const result = await pdfLib(buffer);
 *
 *   // DOCX extraction
 *   const mammothLib = await TextExtractorManager.getMammoth();
 *   const result = await mammothLib.convertToHtml({ buffer });
 *
 *   // XLSX extraction
 *   const ExcelJS = await TextExtractorManager.getExcelJS();
 *   const workbook = new ExcelJS.Workbook();
 *
 * Notes:
 *   - All methods are async and load libraries on-demand
 *   - Libraries are cached after first load for performance
 *   - Each library can be loaded independently
 *   - Clear error messages guide users to install missing dependencies
 */

import { ConfigurationError } from "@wf-agent/types";

/**
 * Type definitions for lazy-loaded libraries
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
type PdfParseModule = any;
type MammothModule = typeof import("mammoth");
type ExcelJSModule = typeof import("exceljs");

/**
 * Library metadata for unified loading logic
 */
interface LibraryInfo<T> {
  name: string;
  packageName: string;
  errorMessage: string;
  installSuggestion: string;
}

/**
 * TextExtractorManager - Manages optional text extraction library instances
 */
export class TextExtractorManager {
  private static pdfParsePromise: Promise<PdfParseModule> | null = null;
  private static mammothPromise: Promise<MammothModule> | null = null;
  private static exceljsPromise: Promise<ExcelJSModule> | null = null;

  /**
   * Generic method to load a library with caching
   * @param promiseRef - Reference to the cached promise
   * @param libraryInfo - Library metadata
   * @returns Promise resolving to the loaded module
   * @throws {ConfigurationError} If the library is not installed
   */
  private static async loadLibrary<T>(
    promiseRef: () => Promise<T> | null,
    setPromiseRef: (promise: Promise<T> | null) => void,
    libraryInfo: LibraryInfo<T>,
  ): Promise<T> {
    const existingPromise = promiseRef();
    if (existingPromise) {
      return existingPromise;
    }

    const newPromise = (async () => {
      try {
        const module = await import(libraryInfo.packageName);
        return module as T;
      } catch {
        throw new ConfigurationError(
          libraryInfo.errorMessage,
          undefined,
          { suggestion: libraryInfo.installSuggestion },
        );
      }
    })();

    setPromiseRef(newPromise);
    return newPromise;
  }

  /**
   * Get pdf-parse instance (lazy-loaded)
   * @returns Promise resolving to pdf-parse module
   * @throws {ConfigurationError} If pdf-parse is not installed
   */
  static async getPdfParse(): Promise<PdfParseModule> {
    return TextExtractorManager.loadLibrary(
      () => TextExtractorManager.pdfParsePromise,
      (promise) => { TextExtractorManager.pdfParsePromise = promise; },
      {
        name: "pdf-parse",
        packageName: "pdf-parse",
        errorMessage: "PDF parsing library not found. Install pdf-parse to extract text from PDF files.",
        installSuggestion: "pnpm add pdf-parse",
      },
    );
  }

  /**
   * Get mammoth instance (lazy-loaded)
   * @returns Promise resolving to mammoth module
   * @throws {ConfigurationError} If mammoth is not installed
   */
  static async getMammoth(): Promise<MammothModule> {
    return TextExtractorManager.loadLibrary(
      () => TextExtractorManager.mammothPromise,
      (promise) => { TextExtractorManager.mammothPromise = promise; },
      {
        name: "mammoth",
        packageName: "mammoth",
        errorMessage: "DOCX conversion library not found. Install mammoth to extract text from DOCX files.",
        installSuggestion: "pnpm add mammoth",
      },
    );
  }

  /**
   * Get exceljs instance (lazy-loaded)
   * @returns Promise resolving to exceljs module
   * @throws {ConfigurationError} If exceljs is not installed
   */
  static async getExcelJS(): Promise<ExcelJSModule> {
    return TextExtractorManager.loadLibrary(
      () => TextExtractorManager.exceljsPromise,
      (promise) => { TextExtractorManager.exceljsPromise = promise; },
      {
        name: "exceljs",
        packageName: "exceljs",
        errorMessage: "Excel processing library not found. Install exceljs to extract text from XLSX files.",
        installSuggestion: "pnpm add exceljs",
      },
    );
  }

  /**
   * Check if pdf-parse has been loaded
   * @returns true if pdf-parse is available
   */
  static hasPdfParse(): boolean {
    return TextExtractorManager.pdfParsePromise !== null;
  }

  /**
   * Check if mammoth has been loaded
   * @returns true if mammoth is available
   */
  static hasMammoth(): boolean {
    return TextExtractorManager.mammothPromise !== null;
  }

  /**
   * Check if exceljs has been loaded
   * @returns true if exceljs is available
   */
  static hasExcelJS(): boolean {
    return TextExtractorManager.exceljsPromise !== null;
  }

  /**
   * Dispose all library instances (useful for testing)
   * After calling, libraries will be reloaded on next access
   */
  static dispose(): void {
    TextExtractorManager.pdfParsePromise = null;
    TextExtractorManager.mammothPromise = null;
    TextExtractorManager.exceljsPromise = null;
  }

  /**
   * Dispose specific library instance
   * @param library - Name of the library to dispose ('pdf-parse' | 'mammoth' | 'exceljs')
   */
  static disposeLibrary(library: "pdf-parse" | "mammoth" | "exceljs"): void {
    switch (library) {
      case "pdf-parse":
        TextExtractorManager.pdfParsePromise = null;
        break;
      case "mammoth":
        TextExtractorManager.mammothPromise = null;
        break;
      case "exceljs":
        TextExtractorManager.exceljsPromise = null;
        break;
    }
  }
}
