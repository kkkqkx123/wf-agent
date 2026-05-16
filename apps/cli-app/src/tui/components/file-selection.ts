/**
 * File Selection Dialog Component
 * Allows users to browse and select files from the filesystem
 */

import { readdir } from "fs/promises";
import { join, resolve, dirname, extname } from "path";
import { Box, Container, Text, SelectList, Input } from "../core/index.js";
import type { Component } from "../core/tui.js";
import { createContextualLogger } from "@wf-agent/sdk/utils";

export interface FileSelectionOptions {
  allowedExtensions?: string[]; // e.g., ['.toml', '.json']
  directory?: string; // Starting directory
  title?: string;
  onSelect?: (filePath: string) => void;
  onCancel?: () => void;
}

interface FileItem {
  value: string;
  label: string;
  description?: string;
  isDirectory: boolean;
  fullPath: string;
}

export class FileSelectionDialog implements Component {
  private container: Container;
  private fileList: SelectList;
  private pathInput: Input;
  private currentDirectory: string;
  private options: FileSelectionOptions;
  private onConfirm?: (filePath: string) => void;
  private onCancel?: () => void;
  private logger = createContextualLogger({ component: "FileSelectionDialog" });

  constructor(options: FileSelectionOptions = {}) {
    this.options = options;
    this.currentDirectory = resolve(options.directory || process.cwd());
    this.onConfirm = options.onSelect;
    this.onCancel = options.onCancel;
    
    this.container = new Container();
    this.fileList = new SelectList([]);
    this.pathInput = new Input("Current path: " + this.currentDirectory);
    
    this.setupLayout();
    this.loadDirectory(this.currentDirectory);
  }

  private setupLayout() {
    // Title
    const title = new Text(this.options.title || "Select File", 1, 0);
    
    // Current path display
    const pathBox = new Box();
    pathBox.addChild(this.pathInput);
    
    // File list
    const listBox = new Box();
    listBox.addChild(this.fileList);
    
    // Help text
    const helpBox = new Box();
    helpBox.addChild(new Text("↑/↓ - Navigate | Enter - Select/Open | .. - Go up | Esc - Cancel"));
    
    this.container.addChild(title);
    this.container.addChild(pathBox);
    this.container.addChild(listBox);
    this.container.addChild(helpBox);
  }

  private async loadDirectory(dirPath: string) {
    try {
      const entries = await readdir(dirPath, { withFileTypes: true });
      const items: FileItem[] = [];
      
      // Add parent directory option if not at root
      const parentDir = dirname(dirPath);
      if (parentDir !== dirPath) {
        items.push({
          value: "..",
          label: "..",
          description: "Parent directory",
          isDirectory: true,
          fullPath: parentDir,
        });
      }
      
      // Add directories and files
      for (const entry of entries) {
        const fullPath = join(dirPath, entry.name);
        
        if (entry.isDirectory()) {
          items.push({
            value: entry.name + "/",
            label: entry.name + "/",
            description: "Directory",
            isDirectory: true,
            fullPath,
          });
        } else {
          // Filter by extension if specified
          const ext = extname(entry.name).toLowerCase();
          if (!this.options.allowedExtensions || this.options.allowedExtensions.includes(ext)) {
            items.push({
              value: entry.name,
              label: entry.name,
              description: `File (${ext})`,
              isDirectory: false,
              fullPath,
            });
          }
        }
      }
      
      // Convert to SelectItem format
      const selectItems = items.map(item => ({
        value: item.value,
        label: item.label,
        description: item.description,
      }));
      
      this.fileList.setItems(selectItems);
      this.currentDirectory = dirPath;
      this.pathInput.setValue(dirPath);
      
    } catch (error) {
      this.logger.error("Failed to read directory", {}, { 
        error: error instanceof Error ? error.message : String(error),
        directory: dirPath 
      });
    }
  }

  handleInput(data: string): void {
    // Handle special keys
    if (data === "\r" || data === "\n") {
      // Enter key - get selected item
      const selectedItem = this.fileList.getSelectedItem?.();
      
      if (selectedItem) {
        // Find the corresponding file item
        const items = this.getFileItems();
        const fileItem = items.find(item => item.value === selectedItem.value);
        
        if (fileItem) {
          if (fileItem.isDirectory) {
            // Navigate into directory
            this.loadDirectory(fileItem.fullPath);
          } else {
            // Select file
            this.onConfirm?.(fileItem.fullPath);
          }
        }
      }
    } else if (data === "\x1b") {
      // Escape key - cancel
      this.onCancel?.();
    } else {
      // Delegate to file list
      this.fileList.handleInput?.(data);
    }
  }

  private getFileItems(): FileItem[] {
    // This would need to track the actual file items
    // For now, return empty array (would need proper state management)
    return [];
  }

  render(width: number): string[] {
    return this.container.render(width);
  }

  invalidate(): void {
    this.container.invalidate();
  }
}
