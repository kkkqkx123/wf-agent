# Special File Format Handling Guide

## Overview

The `read_file` tool is designed for **plain text files only** (code, configs, markdown, etc.). For special format files (PDF, DOCX, XLSX, images, archives), use dedicated extraction tools or scripts.

This guide explains why and how to handle different file formats appropriately.

---

## Why Separate Tools?

### Problems with Mixed Approach

❌ **Using `read_file` for all formats causes:**

1. **Meaningless character/line limits** - PDF pages don't have "lines" in the traditional sense
2. **Loss of structure** - Excel tables become garbled text
3. **No fine-grained control** - Can't select specific pages, sheets, or regions
4. **Poor error messages** - Generic "binary file" errors don't help LLM recover
5. **Bloated tool implementation** - One tool trying to do everything

### Benefits of Dedicated Tools

✅ **Separate tools provide:**

1. **Format-specific parameters** - Page ranges, sheet names, extraction modes
2. **Structured output** - JSON with metadata, page numbers, formatting info
3. **Better error handling** - Specific guidance for each format
4. **LLM control** - Explicit choice of tool based on file type
5. **Clean architecture** - Each tool has a single responsibility

---

## File Format Categories

### 1. Plain Text Files ✅ Use `read_file`

**Supported formats:**
- Code files: `.js`, `.ts`, `.py`, `.java`, `.cpp`, `.go`, `.rs`, etc.
- Config files: `.json`, `.yaml`, `.toml`, `.xml`, `.ini`, `.env`
- Documentation: `.md`, `.txt`, `.rst`
- Web files: `.html`, `.css`, `.vue`, `.svelte`
- Shell scripts: `.sh`, `.bash`, `.ps1`, `.bat`
- Data files: `.csv`, `.sql`, `.log`

**Features:**
- Line numbers for reference
- Slice mode (offset/limit) for large files
- Indentation mode for semantic code blocks
- Character limits to prevent token overflow

**Example:**
```typescript
// Read a TypeScript file
{ path: "src/app.ts" }

// Read specific lines
{ path: "src/app.ts", offset: 50, limit: 100 }

// Extract function containing line 42
{ 
  path: "src/app.ts", 
  mode: "indentation", 
  indentation: { anchor_line: 42 } 
}
```

---

### 2. PDF Files 📄 Use Dedicated Script

**Why not `read_file`:**
- Binary format with complex structure
- Pages, fonts, images, annotations
- Character/line limits are meaningless
- Need page-level control

**Recommended approach:**
```python
# extract_pdf.py
import pdfplumber
import json

def extract_pdf(path: str, pages: str = None, extract_text: bool = True):
    """
    Extract text from PDF with page-level control.
    
    Args:
        path: Path to PDF file
        pages: Page range (e.g., "1-5", "1,3,5", or None for all)
        extract_text: Whether to extract text content
    
    Returns:
        JSON with structured content by page
    """
    with pdfplumber.open(path) as pdf:
        result = {
            "metadata": {
                "title": pdf.metadata.get("Title"),
                "author": pdf.metadata.get("Author"),
                "total_pages": len(pdf.pages)
            },
            "pages": []
        }
        
        # Process specified pages
        page_nums = parse_page_range(pages, len(pdf.pages)) if pages else range(len(pdf.pages))
        
        for page_num in page_nums:
            page = pdf.pages[page_num]
            text = page.extract_text()
            
            result["pages"].append({
                "page_number": page_num + 1,
                "text": text,
                "word_count": len(text.split()) if text else 0
            })
        
        return json.dumps(result, ensure_ascii=False)
```

**Tool configuration:**
```toml
# extract-pdf/schema.toml
[id]
name = "extract_pdf"
type = "script"
category = "filesystem"

[description]
summary = "Extract text and metadata from PDF files"
detailed = """
Extracts text content from PDF files with fine-grained control over:
- Page selection (specific pages or ranges)
- Text extraction mode
- Metadata extraction

Use this tool when you need to read PDF documents.
The read_file tool cannot handle PDF files.
"""

[parameters.path]
type = "string"
required = true
description = "Path to the PDF file"

[parameters.pages]
type = "string"
required = false
description = "Page range to extract (e.g., '1-5', '1,3,5', or omit for all)"
```

**LLM usage:**
```
User: "Read the introduction from report.pdf"

Assistant: I'll extract pages 1-5 from the PDF to get the introduction.

Tool call: extract_pdf(path="report.pdf", pages="1-5")
```

---

### 3. Word Documents (.docx) 📝 Use Dedicated Script

**Why not `read_file`:**
- ZIP-based format with XML internals
- Contains formatting, styles, images, tables
- Need paragraph/table-level access

**Recommended approach:**
```python
# extract_docx.py
from docx import Document
import json

def extract_docx(path: str, include_tables: bool = True):
    """
    Extract content from Word document.
    
    Args:
        path: Path to .docx file
        include_tables: Whether to extract tables
    
    Returns:
        JSON with paragraphs, headings, and optional tables
    """
    doc = Document(path)
    
    result = {
        "metadata": {
            "paragraph_count": len(doc.paragraphs),
            "table_count": len(doc.tables) if include_tables else 0
        },
        "content": []
    }
    
    # Extract paragraphs
    for para in doc.paragraphs:
        if para.text.strip():  # Skip empty paragraphs
            result["content"].append({
                "type": "paragraph",
                "text": para.text,
                "style": para.style.name if para.style else "Normal"
            })
    
    # Extract tables if requested
    if include_tables:
        for i, table in enumerate(doc.tables):
            table_data = []
            for row in table.rows:
                row_data = [cell.text.strip() for cell in row.cells]
                table_data.append(row_data)
            
            result["content"].append({
                "type": "table",
                "table_index": i,
                "data": table_data
            })
    
    return json.dumps(result, ensure_ascii=False)
```

---

### 4. Excel Files (.xlsx) 📊 Use Dedicated Script

**Why not `read_file`:**
- Spreadsheet with multiple sheets
- Formulas, formatting, charts
- Need sheet/cell-level access

**Recommended approach:**
```python
# extract_xlsx.py
import openpyxl
import json

def extract_xlsx(path: str, sheets: list = None, max_rows: int = 1000):
    """
    Extract data from Excel workbook.
    
    Args:
        path: Path to .xlsx file
        sheets: List of sheet names to extract (None for all)
        max_rows: Maximum rows per sheet to prevent huge output
    
    Returns:
        JSON with sheet data as arrays
    """
    wb = openpyxl.load_workbook(path, read_only=True, data_only=True)
    
    result = {
        "metadata": {
            "sheet_names": wb.sheetnames,
            "total_sheets": len(wb.sheetnames)
        },
        "sheets": {}
    }
    
    sheet_names = sheets if sheets else wb.sheetnames
    
    for sheet_name in sheet_names:
        if sheet_name not in wb.sheetnames:
            continue
        
        ws = wb[sheet_name]
        data = []
        
        for i, row in enumerate(ws.iter_rows(values_only=True)):
            if i >= max_rows:
                break
            
            # Convert None to empty string for consistency
            clean_row = [str(cell) if cell is not None else "" for cell in row]
            
            # Skip completely empty rows
            if any(cell.strip() for cell in clean_row):
                data.append(clean_row)
        
        result["sheets"][sheet_name] = {
            "row_count": len(data),
            "data": data
        }
    
    wb.close()
    return json.dumps(result, ensure_ascii=False)
```

---

### 5. Images 🖼️ Use Analysis Script

**Why not `read_file`:**
- Binary pixel data
- Cannot be represented as text
- Need computer vision processing

**Recommended approach:**
```python
# analyze_image.py
from PIL import Image
import base64
import json

def analyze_image(path: str, extract_text: bool = False):
    """
    Analyze image file and extract metadata.
    
    Args:
        path: Path to image file
        extract_text: Whether to perform OCR (requires pytesseract)
    
    Returns:
        JSON with image metadata and optional OCR text
    """
    img = Image.open(path)
    
    result = {
        "metadata": {
            "format": img.format,
            "size": img.size,  # (width, height)
            "mode": img.mode,  # RGB, RGBA, L, etc.
            "file_size_bytes": os.path.getsize(path)
        }
    }
    
    # Optional OCR
    if extract_text:
        try:
            import pytesseract
            text = pytesseract.image_to_string(img)
            result["ocr_text"] = text
        except ImportError:
            result["error"] = "OCR requires pytesseract installation"
    
    return json.dumps(result, ensure_ascii=False)
```

---

### 6. Archives (.zip, .tar, etc.) 📦 Use Archive Tools

**Why not `read_file`:**
- Compressed binary format
- Contains multiple files
- Need listing/extraction capabilities

**Recommended approach:**
```python
# extract_archive.py
import zipfile
import tarfile
import json
import os

def extract_archive(path: str, list_only: bool = True, extract_path: str = None):
    """
    List or extract archive contents.
    
    Args:
        path: Path to archive file
        list_only: If True, only list contents; if False, extract
        extract_path: Where to extract (if list_only=False)
    
    Returns:
        JSON with file listing or extraction status
    """
    result = {"archive_type": None, "files": []}
    
    # Try ZIP
    if zipfile.is_zipfile(path):
        result["archive_type"] = "zip"
        with zipfile.ZipFile(path, 'r') as zf:
            for info in zf.infolist():
                result["files"].append({
                    "name": info.filename,
                    "size": info.file_size,
                    "is_dir": info.filename.endswith('/')
                })
    
    # Try TAR
    elif tarfile.is_tarfile(path):
        result["archive_type"] = "tar"
        with tarfile.open(path, 'r:*') as tf:
            for member in tf.getmembers():
                result["files"].append({
                    "name": member.name,
                    "size": member.size,
                    "is_dir": member.isdir()
                })
    
    else:
        return json.dumps({"error": "Unsupported archive format"})
    
    return json.dumps(result, ensure_ascii=False)
```

---

## Decision Flowchart

```
Need to read a file?
│
├─ Is it a plain text file? (.js, .py, .md, .json, etc.)
│  └─ YES → Use read_file tool ✅
│     ├─ Need specific lines? → Use offset/limit
│     └─ Have line number from error/search? → Use indentation mode
│
├─ Is it a PDF? (.pdf)
│  └─ YES → Use extract_pdf script 📄
│     ├─ Need specific pages? → Specify pages parameter
│     └─ Need full document? → Omit pages parameter
│
├─ Is it a Word document? (.docx)
│  └─ YES → Use extract_docx script 📝
│     └─ Need tables? → Set include_tables=true
│
├─ Is it an Excel file? (.xlsx, .xls)
│  └─ YES → Use extract_xlsx script 📊
│     ├─ Need specific sheets? → Specify sheets parameter
│     └─ Large file? → Set max_rows to limit output
│
├─ Is it an image? (.png, .jpg, .gif, etc.)
│  └─ YES → Use analyze_image script 🖼️
│     └─ Need text from image? → Set extract_text=true (OCR)
│
├─ Is it an archive? (.zip, .tar, .gz, etc.)
│  └─ YES → Use extract_archive script 📦
│     ├─ Just want to see contents? → Set list_only=true
│     └─ Want to extract? → Set list_only=false and specify extract_path
│
└─ Other binary format?
   └─ Use specialized parser or convert to text first
```

---

## Implementation Checklist

For adding support for a new file format:

1. **Create script directory**
   ```
   resources/scripts/extract-{format}/
   ├── handler.py          # Python script
   ├── schema.toml         # Tool definition
   └── description.md      # Usage documentation
   ```

2. **Implement handler**
   - Use appropriate library (pdfplumber, python-docx, openpyxl, etc.)
   - Return structured JSON output
   - Include metadata and content
   - Handle errors gracefully

3. **Define schema**
   - Clear parameter descriptions
   - Required vs optional parameters
   - Default values where appropriate

4. **Write description**
   - When to use this tool
   - What it returns
   - Example usage
   - Comparison to read_file

5. **Update this guide**
   - Add new format section
   - Update decision flowchart
   - Provide example usage

---

## Best Practices

### For LLM Prompts

✅ **Do:**
```
"If you encounter a PDF file, use extract_pdf instead of read_file."
"For Excel files, use extract_xlsx to get structured sheet data."
"When reading code files, read_file with indentation mode is preferred."
```

❌ **Don't:**
```
"Try to read all files with read_file."
"Binary files will be automatically handled."
```

### For Tool Design

✅ **Do:**
- Keep tools focused on one format
- Provide format-specific parameters
- Return structured, parseable output
- Include helpful error messages

❌ **Don't:**
- Mix multiple formats in one tool
- Return unstructured text blobs
- Hide format-specific details
- Give generic error messages

---

## Migration Notes

### From Old Approach

If your system previously used `read_file` for all formats:

1. **Update prompts** to guide LLM toward correct tool
2. **Add error messages** in read_file that suggest alternatives
3. **Gradually introduce** dedicated scripts
4. **Monitor usage** to see which formats are most common
5. **Prioritize** implementing scripts for frequently-used formats

### Backward Compatibility

The simplified `read_file` maintains compatibility for:
- All plain text files ✅
- Code files with line numbers ✅
- Slice and indentation modes ✅

It now **rejects** (with helpful guidance):
- PDF files ❌ → Use extract_pdf
- Word documents ❌ → Use extract_docx
- Excel files ❌ → Use extract_xlsx
- Images ❌ → Use analyze_image
- Other binaries ❌ → Use appropriate tool

---

## Summary

| Format | Tool | Reason |
|--------|------|--------|
| Text/Code | `read_file` | Simple, fast, line-based |
| PDF | `extract_pdf` | Page-level control, structured output |
| DOCX | `extract_docx` | Paragraph/table access |
| XLSX | `extract_xlsx` | Sheet/cell-level access |
| Images | `analyze_image` | Computer vision processing |
| Archives | `extract_archive` | Listing/extraction capabilities |

**Key principle:** Right tool for the right job. Don't force text-based reading on non-text formats.
