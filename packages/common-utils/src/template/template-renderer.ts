/**
 * TemplateRenderer - 模板渲染器
 * 提供模板变量替换功能，支持嵌套路径解析
 *
 * 功能：
 * - 支持 {{variable}} 占位符替换
 * - 支持嵌套路径解析（如 user.name）
 * - 支持数组索引访问（如 items[0].name）
 * - 支持条件渲染 {{#if variable}}...{{/if}}
 * - 支持循环渲染 {{#each array}}...{{/each}}
 * - 提供安全的变量值获取
 */

import { resolvePath } from "../evalutor/path-resolver.js";

/**
 * Supported Loop-Specific Variables
 * These variables can only be used inside {{#each}} loops.
 */
const SUPPORTED_LOOP_SPECIAL_VARS = new Set(["@index", "@first", "@last"]);

/**
 * Checks if it is a supported loop-specific variable
 * @param variableName variable name
 * @returns Whether the variable is a supported loop special variable.
 */
function isSupportedLoopSpecialVar(variableName: string): boolean {
  return SUPPORTED_LOOP_SPECIAL_VARS.has(variableName);
}

/**
 * Checks if it is a loop special variable (starts with @)
 * @param variableName variableName
 * @returns if the variable is loop-specific
 */
function isLoopSpecialVar(variableName: string): boolean {
  return variableName.startsWith("@");
}

/**
 * Check if this is the variable
 * @param variableName variable name
 * @returns if the variable is this
 */
function isThisVariable(variableName: string): boolean {
  return variableName === "this" || variableName.startsWith("this.");
}

/**
 * Template Rendering Error Class
 */
export class TemplateRenderError extends Error {
  constructor(
    message: string,
    public readonly variableName: string,
    public readonly context?: string,
  ) {
    super(message);
    this.name = "TemplateRenderError";
  }
}

// Variable value type is defined inline where needed

/**
 * Get variable values
 * @param variableName variable name, supports nested paths (such as user.name)
 * @param variables object
 * @returns variable value, undefined if it does not exist
 */
export function getVariableValue(variableName: string, variables: Record<string, unknown>): unknown {
  if (!variableName || !variables) {
    return undefined;
  }

  // Trying to get direct access to
  if (variableName in variables) {
    return variables[variableName];
  }

  // Try path resolution
  return resolvePath(variableName, variables);
}

/**
 * Check if value is true (for conditional rendering)
 * @param value The value to check
 * @returns whether the value is true or not
 */
function isTruthy(value: unknown): boolean {
  if (value === undefined || value === null) {
    return false;
  }
  if (typeof value === "boolean") {
    return value;
  }
  if (typeof value === "number") {
    return value !== 0;
  }
  if (typeof value === "string") {
    return value.length > 0;
  }
  if (Array.isArray(value)) {
    return value.length > 0;
  }
  return true;
}

/**
 * Parse variables in the form this.xxx
 * @param variableName variable name
 * @param thisValue of this
 * @returns variable value
 */
function resolveThisVariable(variableName: string, thisValue: unknown): unknown {
  if (variableName === "this") {
    return thisValue;
  }
  if (variableName.startsWith("this.")) {
    const prop = variableName.substring(5); // 'this.'.length = 5
    return getVariableValue(prop, thisValue as Record<string, unknown>);
  }
  return undefined;
}

/**
 * Rendering conditional blocks {{#if variable}}... {{/if}}
 * @param template Template string
 * @param variables Variable objects
 * @param thisValue optional this value (for loop context)
 * @param inLoop Whether the loop is inside a loop or not.
 * @returns Rendered string
 */
function renderConditionals(
  template: string,
  variables: Record<string, unknown>,
  thisValue?: unknown,
  inLoop: boolean = false,
): string {
  // Match {{#if variable}}... {{/if}} pattern
  const ifRegex = /\{\{#if\s+(\S+?)\}\}([\s\S]*?)\{\{\/if\}\}/g;

  return template.replace(ifRegex, (match, variableName, content) => {
    const trimmedName = variableName.trim();
    let value: unknown;

    // Handling of loop-specific variables
    if (isLoopSpecialVar(trimmedName)) {
      if (!inLoop) {
        throw new TemplateRenderError(
          `The loop special variable '${trimmedName}' can only be used inside a {{#each}} loop`,
          trimmedName,
          "{{#if}}",
        );
      }
      if (!isSupportedLoopSpecialVar(trimmedName)) {
        throw new TemplateRenderError(
          `不支持的循环特殊变量 '${trimmedName}'。支持的变量: ${Array.from(SUPPORTED_LOOP_SPECIAL_VARS).join(", ")}`,
          trimmedName,
          "{{#if}}",
        );
      }
      value = getVariableValue(trimmedName, variables);
    }
    // Handle the variable in this.xxx format.
    else if (isThisVariable(trimmedName)) {
      if (!inLoop) {
        throw new TemplateRenderError(
          `The '${trimmedName}' variable can only be used inside the {{#each}} loop`,
          trimmedName,
          "{{#if}}",
        );
      }
      value = resolveThisVariable(trimmedName, thisValue);
    } else {
      value = getVariableValue(trimmedName, variables);
    }

    if (isTruthy(value)) {
      return content;
    }
    return "";
  });
}

/**
 * Render loop block {{#each array}}... {{/each}}
 * @param template Template string
 * @param variables
 * @returns Rendered string
 */
function renderLoops(template: string, variables: Record<string, unknown>): string {
  // Match {{#each array}}... {{/each}} mode
  const eachRegex = /\{\{#each\s+(\S+?)\}\}([\s\S]*?)\{\{\/each\}\}/g;

  return template.replace(eachRegex, (match, variableName, content) => {
    const trimmedName = variableName.trim();
    const value = getVariableValue(trimmedName, variables);

    if (!Array.isArray(value) || value.length === 0) {
      return "";
    }

    // Render each element
    const results: string[] = [];
    for (let i = 0; i < value.length; i++) {
      const item = value[i];
      const isFirst = i === 0;
      const isLast = i === value.length - 1;

      // Creating a context with this and @index
      const itemContext: Record<string, unknown> = {
        ...variables,
        this: item,
        "@index": i,
        "@first": isFirst,
        "@last": isLast,
      };

      // Renders the contents of the current item
      let itemContent = content;

      // 1. first deal with conditional rendering inside the loop (pass in this value, mark inside the loop)
      itemContent = renderConditionals(itemContent, itemContext, item, true);

      // 2. Replace {{this}} or {{this.property}}
      itemContent = itemContent.replace(
        /\{\{this(?:\.(\S+?))?\}\}/g,
        (m: string, prop?: string) => {
          if (prop) {
            const propValue = getVariableValue(prop, item);
            return propValue !== undefined && propValue !== null ? String(propValue) : m;
          }
          return item !== undefined && item !== null ? String(item) : m;
        },
      );

      // 3. Replacement of loop-specific variables
      itemContent = itemContent.replace(
        /\{\{(@[a-zA-Z_][a-zA-Z0-9_]*)\}\}/g,
        (m: string, varName: string) => {
          if (!isSupportedLoopSpecialVar(varName)) {
            throw new TemplateRenderError(
              `不支持的循环特殊变量 '${varName}'。支持的变量: ${Array.from(SUPPORTED_LOOP_SPECIAL_VARS).join(", ")}`,
              varName,
              "{{#each}}",
            );
          }
          return String(itemContext[varName]);
        },
      );

      // 4. Replacement of other variables
      itemContent = renderSimpleVariables(itemContent, itemContext, true);

      results.push(itemContent);
    }

    return results.join("");
  });
}

/**
 * Rendering simple variables {{variable}}
 * @param template template string
 * @param variables Variable objects
 * @param inLoop Whether the loop is inside a loop or not.
 * @returns Rendered string
 */
function renderSimpleVariables(
  template: string,
  variables: Record<string, unknown>,
  inLoop: boolean = false,
): string {
  if (!template) {
    return "";
  }

  if (!variables || Object.keys(variables).length === 0) {
    // Checking for illegally used special variables
    checkInvalidSpecialVars(template, inLoop);
    return template;
  }

  // Use regular expressions to match {{variable}} placeholders (excluding those beginning with {{# and {{/))
  const placeholderRegex = /\{\{([^#/][^}]*?)\}\}/g;

  return template.replace(placeholderRegex, (match, variableName) => {
    // Remove whitespace characters from the ends of variable names
    const trimmedName = variableName.trim();

    // Checking Loop Special Variables
    if (isLoopSpecialVar(trimmedName)) {
      if (!inLoop) {
        throw new TemplateRenderError(
          `The loop special variable '${trimmedName}' can only be used inside a {{#each}} loop`,
          trimmedName,
          "variable",
        );
      }
      if (!isSupportedLoopSpecialVar(trimmedName)) {
        throw new TemplateRenderError(
          `不支持的循环特殊变量 '${trimmedName}'。支持的变量: ${Array.from(SUPPORTED_LOOP_SPECIAL_VARS).join(", ")}`,
          trimmedName,
          "variable",
        );
      }
      // Supported special variables should already be replaced in the loop processing, if it comes to this it means something is wrong
      return match;
    }

    // Checking this variable
    if (isThisVariable(trimmedName)) {
      if (!inLoop) {
        throw new TemplateRenderError(
          `The '${trimmedName}' variable can only be used inside the {{#each}} loop`,
          trimmedName,
          "variable",
        );
      }
      // The this variable should already have been replaced in the loop processing, if it gets here something is wrong
      return match;
    }

    const value = getVariableValue(trimmedName, variables);

    // If the value is undefined or null, the original placeholder is preserved.
    if (value === undefined || value === null) {
      return match;
    }

    // Converting values to strings
    return String(value);
  });
}

/**
 * Checking for illegal use of special variables in templates
 * @param template Template string
 * @param inLoop Whether the loop is inside a loop or not.
 */
function checkInvalidSpecialVars(template: string, inLoop: boolean): void {
  const placeholderRegex = /\{\{([^#/][^}]*?)\}\}/g;
  let match;

  while ((match = placeholderRegex.exec(template)) !== null) {
    const variableName = match[1]?.trim();
    if (!variableName) continue;

    if (isLoopSpecialVar(variableName)) {
      if (!inLoop) {
        throw new TemplateRenderError(
          `The loop special variable '${variableName}' can only be used inside the {{#each}} loop`,
          variableName,
          "variable",
        );
      }
      if (!isSupportedLoopSpecialVar(variableName)) {
        throw new TemplateRenderError(
          `不支持的循环特殊变量 '${variableName}'。支持的变量: ${Array.from(SUPPORTED_LOOP_SPECIAL_VARS).join(", ")}`,
          variableName,
          "variable",
        );
      }
    }

    if (isThisVariable(variableName) && !inLoop) {
      throw new TemplateRenderError(
        `The '${variableName}' variable can only be used inside the {{#each}} loop`,
        variableName,
        "variable",
      );
    }
  }
}

/**
 * 渲染模板
 * 替换模板中的 {{variable}} 占位符，支持条件和循环
 *
 * @param template 模板字符串，包含 {{variable}} 占位符
 * @param variables 变量对象
 * @returns 渲染后的字符串
 * @throws {TemplateRenderError} 当使用不支持的循环特殊变量或在循环外使用特殊变量时抛出
 *
 * @example
 * ```ts
 * const template = 'Hello, {{name}}! Today is {{date}}.';
 * const result = renderTemplate(template, { name: 'Alice', date: '2024-01-01' });
 * // 结果: 'Hello, Alice! Today is 2024-01-01.'
 * ```
 *
 * @example
 * ```ts
 * const template = 'User: {{user.name}}, Age: {{user.age}}';
 * const result = renderTemplate(template, { user: { name: 'Bob', age: 30 } });
 * // 结果: 'User: Bob, Age: 30'
 * ```
 *
 * @example 条件渲染
 * ```ts
 * const template = '{{#if showName}}Name: {{name}}{{/if}}';
 * const result = renderTemplate(template, { showName: true, name: 'Alice' });
 * // 结果: 'Name: Alice'
 * ```
 *
 * @example 循环渲染
 * ```ts
 * const template = 'Items:{{#each items}} - {{this}}{{/each}}';
 * const result = renderTemplate(template, { items: ['A', 'B', 'C'] });
 * // 结果: 'Items: - A - B - C'
 * ```
 */
export function renderTemplate(template: string, variables: Record<string, unknown>): string {
  if (!template) {
    return "";
  }

  if (!variables) {
    variables = {};
  }

  let result = template;

  // 1. Handle loop rendering first (conditional rendering is handled inside the loop)
  result = renderLoops(result, variables);

  // 2. Re-processing of the top-level conditional rendering
  result = renderConditionals(result, variables);

  // 3. Finalizing simple variable substitution
  result = renderSimpleVariables(result, variables);

  return result;
}

/**
 * Batch Rendering Templates
 * Batch rendering of multiple templates
 *
 * @param templates array of templates
 * @param variables Variable objects
 * @returns array of rendered strings
 */
export function renderTemplates(templates: string[], variables: Record<string, unknown>): string[] {
  return templates.map(template => renderTemplate(template, variables));
}

/**
 * Validating template variables
 * Checks that all variables in the template exist in the variable object
 *
 * @param template Template string
 * @param variables Variable objects
 * @returns an array containing the names of the missing variables, or an empty array if none are missing
 */
export function validateTemplateVariables(
  template: string,
  variables: Record<string, unknown>,
): string[] {
  const placeholderRegex = /\{\{([^}]+)\}\}/g;
  const missingVariablesSet = new Set<string>();
  let match;

  while ((match = placeholderRegex.exec(template)) !== null) {
    const variableName = match[1]?.trim();
    if (!variableName) continue;

    // Skip control structure
    if (variableName.startsWith("#") || variableName.startsWith("/")) continue;

    // Skip loop special variables and this variables (they are dynamically generated at runtime)
    if (isLoopSpecialVar(variableName) || isThisVariable(variableName)) continue;

    const value = getVariableValue(variableName, variables);

    if (value === undefined || value === null) {
      missingVariablesSet.add(variableName);
    }
  }

  return Array.from(missingVariablesSet);
}
