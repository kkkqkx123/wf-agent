function resolvePath(path: string, root: unknown): unknown {
  if (!path || !root) {
    return undefined;
  }
  const parts = path.split(".");
  let value: unknown = root;
  for (const part of parts) {
    if (value === null || value === undefined) {
      return undefined;
    }
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

const SUPPORTED_LOOP_SPECIAL_VARS = new Set(["@index", "@first", "@last"]);

function isSupportedLoopSpecialVar(variableName: string): boolean {
  return SUPPORTED_LOOP_SPECIAL_VARS.has(variableName);
}

function isLoopSpecialVar(variableName: string): boolean {
  return variableName.startsWith("@");
}

function isThisVariable(variableName: string): boolean {
  return variableName === "this" || variableName.startsWith("this.");
}

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

export function getVariableValue(
  variableName: string,
  variables: Record<string, unknown>,
): unknown {
  if (!variableName || !variables) {
    return undefined;
  }

  if (variableName in variables) {
    return variables[variableName];
  }

  return resolvePath(variableName, variables);
}

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

function resolveThisVariable(variableName: string, thisValue: unknown): unknown {
  if (variableName === "this") {
    return thisValue;
  }
  if (variableName.startsWith("this.")) {
    const prop = variableName.substring(5);
    return getVariableValue(prop, thisValue as Record<string, unknown>);
  }
  return undefined;
}

function renderConditionals(
  template: string,
  variables: Record<string, unknown>,
  thisValue?: unknown,
  inLoop: boolean = false,
): string {
  const ifRegex = /\{\{#if\s+(\S+?)\}\}([\s\S]*?)\{\{\/if\}\}/g;

  return template.replace(ifRegex, (_match, variableName, content) => {
    const trimmedName = variableName.trim();
    let value: unknown;

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
          `Unsupported loop special variables '${trimmedName}'. Supported variables: ${Array.from(SUPPORTED_LOOP_SPECIAL_VARS).join(", ")}`,
          trimmedName,
          "{{#if}}",
        );
      }
      value = getVariableValue(trimmedName, variables);
    } else if (isThisVariable(trimmedName)) {
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

function renderLoops(template: string, variables: Record<string, unknown>): string {
  const eachRegex = /\{\{#each\s+(\S+?)\}\}([\s\S]*?)\{\{\/each\}\}/g;

  return template.replace(eachRegex, (_match, variableName, content) => {
    const trimmedName = variableName.trim();
    const value = getVariableValue(trimmedName, variables);

    if (!Array.isArray(value) || value.length === 0) {
      return "";
    }

    const results: string[] = [];
    for (let i = 0; i < value.length; i++) {
      const item = value[i];
      const isFirst = i === 0;
      const isLast = i === value.length - 1;

      const itemContext: Record<string, unknown> = {
        ...variables,
        this: item,
        "@index": i,
        "@first": isFirst,
        "@last": isLast,
      };

      let itemContent = content;

      itemContent = renderConditionals(itemContent, itemContext, item, true);

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

      itemContent = itemContent.replace(
        /\{\{(@[a-zA-Z_][a-zA-Z0-9_]*)\}\}/g,
        (_m: string, varName: string) => {
          if (!isSupportedLoopSpecialVar(varName)) {
            throw new TemplateRenderError(
              `Unsupported loop special variable '${varName}'. Supported variables: ${Array.from(SUPPORTED_LOOP_SPECIAL_VARS).join(", ")}`,
              varName,
              "{{#each}}",
            );
          }
          return String(itemContext[varName]);
        },
      );

      itemContent = renderSimpleVariables(itemContent, itemContext, true);

      results.push(itemContent);
    }

    return results.join("");
  });
}

function renderSimpleVariables(
  template: string,
  variables: Record<string, unknown>,
  inLoop: boolean = false,
): string {
  if (!template) {
    return "";
  }

  if (!variables || Object.keys(variables).length === 0) {
    checkInvalidSpecialVars(template, inLoop);
    return template;
  }

  const placeholderRegex = /\{\{([^#/][^}]*?)\}\}/g;

  return template.replace(placeholderRegex, (match, variableName) => {
    const trimmedName = variableName.trim();

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
          `Unsupported loop special variable '${trimmedName}'. Supported variables: ${Array.from(SUPPORTED_LOOP_SPECIAL_VARS).join(", ")}`,
          trimmedName,
          "variable",
        );
      }
      return match;
    }

    if (isThisVariable(trimmedName)) {
      if (!inLoop) {
        throw new TemplateRenderError(
          `The '${trimmedName}' variable can only be used inside the {{#each}} loop`,
          trimmedName,
          "variable",
        );
      }
      return match;
    }

    const value = getVariableValue(trimmedName, variables);

    if (value === undefined || value === null) {
      return match;
    }

    return String(value);
  });
}

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
          `Unsupported loop special variable '${variableName}'. Supported variables: ${Array.from(SUPPORTED_LOOP_SPECIAL_VARS).join(", ")}`,
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

export function renderTemplate(template: string, variables: Record<string, unknown>): string {
  if (!template) {
    return "";
  }

  if (!variables) {
    variables = {};
  }

  let result = template;

  result = renderLoops(result, variables);
  result = renderConditionals(result, variables);
  result = renderSimpleVariables(result, variables);

  return result;
}

export function renderTemplates(templates: string[], variables: Record<string, unknown>): string[] {
  return templates.map(template => renderTemplate(template, variables));
}

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

    if (variableName.startsWith("#") || variableName.startsWith("/")) continue;

    if (isLoopSpecialVar(variableName) || isThisVariable(variableName)) continue;

    const value = getVariableValue(variableName, variables);

    if (value === undefined || value === null) {
      missingVariablesSet.add(variableName);
    }
  }

  return Array.from(missingVariablesSet);
}
