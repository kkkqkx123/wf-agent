/**
 * PathResolver - 路径解析器
 * 提供简单的对象路径解析功能
 *
 * 支持的路径格式：
 * - 嵌套对象访问：如 "user.name"、"output.data.items"
 * - 数组索引访问：如 "items[0]"、"items[0].name"
 * - 组合访问：如 "output.data.items[0].name"
 *
 * 使用示例：
 * - resolvePath("user.name", obj) - 获取嵌套属性值
 * - resolvePath("items[0].name", obj) - 获取数组元素属性
 * - setPath("user.name", obj, "John") - 设置嵌套属性值
 * - pathExists("user.name", obj) - 检查路径是否存在
 */

import { validatePath } from "./security-validator.js";
/**
 * 解析路径并获取值
 * @param path 路径字符串，支持嵌套访问和数组索引，如 "user.name"、"items[0].name"
 * @param root 根对象
 * @returns 路径对应的值，如果路径不存在则返回undefined
 */
export function resolvePath(path: string, root: unknown): unknown {
  if (!path || !root) {
    return undefined;
  }

  // Verify path security
  validatePath(path);

  // 支持嵌套路径访问，如 "output.data.items[0].name"
  const parts = path.split(".");
  let value: unknown = root;

  for (const part of parts) {
    if (value === null || value === undefined) {
      return undefined;
    }

    // 处理数组索引访问，如 items[0]
    const arrayMatch = part.match(/(\w+)\[(\d+)\]/);
    if (arrayMatch && arrayMatch[1] && arrayMatch[2]) {
      const arrayName = arrayMatch[1];
      const index = parseInt(arrayMatch[2], 10);
      value = (value as Record<string, unknown>)[arrayName];
      if (Array.isArray(value)) {
        value = value[index];
      }
    } else {
      value = (value as Record<string, unknown>)[part];
    }
  }

  return value;
}

/**
 * Checking if a path exists
 * @param path path string
 * @param root The root object
 * @returns if the path exists
 */
export function pathExists(path: string, root: unknown): boolean {
  try {
    return resolvePath(path, root) !== undefined;
  } catch {
    return false;
  }
}

/**
 * Set the value of path
 * @param path path string
 * @param root The root object
 * @param value The value to set.
 * @returns Whether the setting was successful or not
 */
export function setPath(path: string, root: unknown, value: unknown): boolean {
  if (!path || !root) {
    return false;
  }

  // First check if the path contains empty parts (e.g. "valid..invalid")
  const parts = path.split(".");
  for (const part of parts) {
    if (!part) {
      return false; // Contains empty parts
    }
  }

  // Verify path security (property names beginning with a number throw an exception)
  validatePath(path);
  if (parts.length === 0) {
    return false;
  }

  let current: unknown = root;

  // Iterate to the penultimate level
  for (let i = 0; i < parts.length - 1; i++) {
    const part = parts[i];
    if (!part) {
      return false; // Include empty parts, e.g. "valid...invalid"
    }

    // Handling Array Index Access
    const arrayMatch = part.match(/(\w+)\[(\d+)\]/);
    if (arrayMatch && arrayMatch[1] && arrayMatch[2]) {
      const arrayName = arrayMatch[1];
      const index = parseInt(arrayMatch[2], 10);

      if (!(current as Record<string, unknown>)[arrayName]) {
        (current as Record<string, unknown>)[arrayName] = [];
      }
      current = (current as Record<string, unknown>)[arrayName];

      if (!Array.isArray(current)) {
        return false;
      }

      // If the index is out of range, extend the array
      while ((current as unknown[]).length <= index) {
        (current as unknown[]).push({});
      }
      current = (current as unknown[])[index];
    } else {
      if (!(current as Record<string, unknown>)[part]) {
        (current as Record<string, unknown>)[part] = {};
      }
      current = (current as Record<string, unknown>)[part];
    }
  }

  // Setting the value of the last layer
  const lastPart = parts[parts.length - 1];
  if (!lastPart) {
    return false;
  }

  const arrayMatch = lastPart.match(/(\w+)\[(\d+)\]/);
  if (arrayMatch && arrayMatch[1] && arrayMatch[2]) {
    const arrayName = arrayMatch[1];
    const index = parseInt(arrayMatch[2], 10);

    if (!(current as Record<string, unknown>)[arrayName]) {
      (current as Record<string, unknown>)[arrayName] = [];
    }

    // If the index is out of range, extend the array
    while (((current as Record<string, unknown>)[arrayName] as unknown[]).length <= index) {
      ((current as Record<string, unknown>)[arrayName] as unknown[]).push(undefined);
    }
    ((current as Record<string, unknown>)[arrayName] as unknown[])[index] = value;
  } else {
    (current as Record<string, unknown>)[lastPart] = value;
  }

  return true;
}
