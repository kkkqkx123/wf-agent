#!/usr/bin/env node

/**
 * Modular Agent CLI 可执行文件
 * 这个文件作为 CLI 的入口点
 */

/* global console, process */

import('../dist/index.js').catch((error) => {
  console.error('Failed to start CLI:', error);
  process.exit(1);
});