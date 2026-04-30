# Text Extractor Manager Implementation

## Overview

Created `TextExtractorManager` to manage optional text extraction library dependencies (pdf-parse, mammoth, exceljs) using lazy loading pattern similar to `TomlParserManager`.

## Problem

The original implementation had direct dynamic imports in `text-extractor.ts`, which caused:
1. TypeScript compilation errors when libraries weren't installed
2. No centralized error handling
3. Unclear installation instructions for users
4. Difficult to test and manage library lifecycle

## Solution

Implemented `TextExtractorManager` that provides:
- **Lazy Loading**: Libraries are only loaded when first needed
- **Singleton Pattern**: Each library is loaded once and cached
- **Clear Error Messages**: Helpful messages with installation commands
- **Type Safety**: Proper TypeScript types for all libraries
- **Test-Friendly**: Easy to dispose and reset for testing

## Files Created/Modified

### 1. New File: `sdk/utils/misc/text-extractor-manager.ts`

**Key Features:**
```typescript
class TextExtractorManager {
  // Lazy-loaded getters
  static async getPdfParse(): Promise<PdfParseModule>
  static async getMammoth(): Promise<MammothModule>
  static async getExcelJS(): Promise<ExcelJSModule>
  
  // Availability checks
  static hasPdfParse(): boolean
  static hasMammoth(): boolean
  static hasExcelJS(): boolean
  
  // Lifecycle management
  static dispose(): void
  static disposeLibrary(library: "pdf-parse" | "mammoth" | "exceljs"): void
}
```

**Error Handling Example:**
```typescript
if (!installed) {
  throw new ConfigurationError(
    "PDF parsing library not found. Install pdf-parse to extract text from PDF files.",
    undefined,
    { suggestion: "pnpm add pdf-parse" }
  );
}
```

### 2. Modified: `sdk/utils/misc/text-extractor.ts`

**Changes:**
- Added import: `import { TextExtractorManager } from "./text-extractor-manager.js";`
- Replaced direct imports with manager calls:
  ```typescript
  // Before:
  const pdfParse = await import("pdf-parse");
  
  // After:
  const pdfParse = await TextExtractorManager.getPdfParse();
  ```
- Updated PDF extraction to use buffer-based API (correct for pdf-parse v2.x)

### 3. Modified: `sdk/utils/misc/index.ts`

**Added Export:**
```typescript
export {
  TextExtractorManager,
} from "./text-extractor-manager.js";
```

## Dependencies

The following optional dependencies are declared in `sdk/package.json` under `optionalDependencies`:

```json
{
  "optionalDependencies": {
    "pdf-parse": "^2.4.5",
    "mammoth": "^1.12.0",
    "exceljs": "^4.4.0"
  }
}
```

**Note:** These are marked as `optionalDependencies` because:
- They're only needed for binary file text extraction (PDF, DOCX, XLSX)
- The SDK core functionality works without them
- Users who don't need binary extraction can skip installing them
- Clear error messages guide installation when needed

### Installation

**For full SDK with all features:**
```bash
cd sdk && pnpm install
```
This will install all optional dependencies automatically.

**For minimal installation (skip optional deps):**
```bash
cd sdk && pnpm install --no-optional
```
Then install specific libraries as needed:
```bash
pnpm add pdf-parse    # For PDF extraction
pnpm add mammoth      # For DOCX extraction
pnpm add exceljs      # For XLSX extraction
```

**Large Binary Dependencies:**
These packages include large binary files:
- `pdfjs-dist@5.4.296` (~9.73 MB) - PDF rendering engine
- `@napi-rs/canvas-win32-x64-msvc@0.1.80` (~14.76 MB) - Canvas rendering

Total additional size: ~24-30 MB depending on platform.

## Usage Examples

### Basic Usage

```typescript
import { TextExtractorManager } from '@wf-agent/sdk/utils/misc';

// PDF Extraction
const pdfParse = await TextExtractorManager.getPdfParse();
const dataBuffer = await fs.readFile('document.pdf');
const result = await pdfParse(dataBuffer);
console.log(result.text);

// DOCX Extraction
const mammoth = await TextExtractorManager.getMammoth();
const result = await mammoth.extractRawText({ path: 'document.docx' });
console.log(result.value);

// XLSX Extraction
const ExcelJS = await TextExtractorManager.getExcelJS();
const workbook = new ExcelJS.Workbook();
await workbook.xlsx.readFile('spreadsheet.xlsx');
```

### Using High-Level API

```typescript
import { extractTextFromBinary } from '@wf-agent/sdk/utils/misc';

// Automatically uses TextExtractorManager internally
const result = await extractTextFromBinary('/path/to/file.pdf', '.pdf');

if (result.success) {
  console.log(`Extracted ${result.lineCount} lines`);
  console.log(result.content);
} else {
  console.error(result.error);
}
```

### Testing

```typescript
import { TextExtractorManager } from '@wf-agent/sdk/utils/misc';

// Reset all libraries before test
beforeEach(() => {
  TextExtractorManager.dispose();
});

// Test specific library
test('PDF extraction', async () => {
  expect(TextExtractorManager.hasPdfParse()).toBe(false);
  
  const pdfParse = await TextExtractorManager.getPdfParse();
  expect(TextExtractorManager.hasPdfParse()).toBe(true);
  
  // ... perform tests ...
  
  // Clean up
  TextExtractorManager.disposeLibrary('pdf-parse');
});
```

### Error Handling

```typescript
try {
  const pdfParse = await TextExtractorManager.getPdfParse();
  // Use pdfParse...
} catch (error) {
  if (error instanceof ConfigurationError) {
    console.log(error.message);
    console.log('Suggestion:', error.suggestion);
    // Output: "pnpm add pdf-parse"
  }
}
```

## Benefits

1. **No Compilation Errors**: Type definitions use conditional imports
2. **Clear User Guidance**: Installation suggestions on missing dependencies
3. **Performance**: Libraries loaded once and cached
4. **Flexibility**: Each library can be loaded independently
5. **Testability**: Easy to mock and reset for unit tests
6. **Maintainability**: Centralized library management logic

## Architecture Pattern

This follows the same pattern as `TomlParserManager`:

```
┌─────────────────────┐
│  Application Code   │
└──────────┬──────────┘
           │
           ▼
┌─────────────────────┐
│ TextExtractorManager│ ← Singleton, lazy loading
│  - getPdfParse()    │
│  - getMammoth()     │
│  - getExcelJS()     │
└──────────┬──────────┘
           │
           ▼
┌─────────────────────┐
│ Optional Libraries  │
│  - pdf-parse        │
│  - mammoth          │
│  - exceljs          │
└─────────────────────┘
```

## Migration Notes

### For Developers

- **No breaking changes** to public API
- `extractTextFromBinary()` works exactly as before
- Internal implementation now uses manager pattern

### For Users

- Must install optional dependencies:
  ```bash
  pnpm add pdf-parse mammoth exceljs
  ```
- Clear error messages guide installation if forgotten
- Libraries load automatically on first use

## Testing Strategy

1. **Unit Tests**: Mock TextExtractorManager methods
2. **Integration Tests**: Test actual library loading
3. **Error Tests**: Verify helpful error messages
4. **Lifecycle Tests**: Test dispose and reload functionality

## Future Enhancements

Potential improvements:
- Add support for more formats (RTF, ODT, etc.)
- Implement progress callbacks for large files
- Add caching layer for repeated extractions
- Support streaming extraction for very large files
