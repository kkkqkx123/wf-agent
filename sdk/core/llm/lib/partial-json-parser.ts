/**
 * Partial JSON Parser
 *
 * Borrowed from the Anthropic SDK for parsing incomplete JSON strings.
 * Able to handle partial JSON data in streams, returning usable parsed results.
 *
 * How it works:
 * 1. Tokenize: parses the input string into a stream of tokens.
 * 2. Strip: remove trailing tokens that may cause parsing failure
 * 3. Unstrip: auto-completes the closure symbols.
 * 4. Generate: regenerate the legal JSON string
 * 5. Parse: Calls JSON.parse to parse the string.
 */

type Token = {
  type: string;
  value: string;
};

/**
 * Parse the input string into a stream of tokens.
 */
function tokenize(input: string): Token[] {
  let current = 0;
  const tokens: Token[] = [];

  while (current < input.length) {
    let char = input[current];

    if (char === "\\") {
      current++;
      continue;
    }

    if (char === "{") {
      tokens.push({ type: "brace", value: "{" });
      current++;
      continue;
    }

    if (char === "}") {
      tokens.push({ type: "brace", value: "}" });
      current++;
      continue;
    }

    if (char === "[") {
      tokens.push({ type: "paren", value: "[" });
      current++;
      continue;
    }

    if (char === "]") {
      tokens.push({ type: "paren", value: "]" });
      current++;
      continue;
    }

    if (char === ":") {
      tokens.push({ type: "separator", value: ":" });
      current++;
      continue;
    }

    if (char === ",") {
      tokens.push({ type: "delimiter", value: "," });
      current++;
      continue;
    }

    if (char === '"') {
      let value = "";
      let danglingQuote = false;

      char = input[++current];

      while (char !== '"') {
        if (current === input.length) {
          danglingQuote = true;
          break;
        }

        if (char === "\\") {
          current++;
          if (current === input.length) {
            danglingQuote = true;
            break;
          }
          const nextChar = input[current];
          switch (nextChar) {
            case '"':
              value += '"';
              break;
            case "\\":
              value += "\\";
              break;
            case "/":
              value += "/";
              break;
            case "b":
              value += "\b";
              break;
            case "f":
              value += "\f";
              break;
            case "n":
              value += "\n";
              break;
            case "r":
              value += "\r";
              break;
            case "t":
              value += "\t";
              break;
            case "u": {
              // Read 4 hex digits for unicode escape
              let hex = "";
              for (let i = 0; i < 4; i++) {
                current++;
                if (current === input.length) {
                  danglingQuote = true;
                  break;
                }
                hex += input[current];
              }
              if (!danglingQuote) {
                value += String.fromCharCode(parseInt(hex, 16));
              }
              break;
            }
            default:
              // For unknown escape sequences, preserve the raw characters
              value += "\\" + nextChar;
          }
          char = input[++current];
        } else {
          value += char;
          char = input[++current];
        }
      }

      // Skip past the closing quote (required to avoid re-reading it as a new string)
      current++;

      if (!danglingQuote) {
        tokens.push({ type: "string", value });
      }
      continue;
    }

    const WHITESPACE = /\s/;
    if (char && WHITESPACE.test(char)) {
      current++;
      continue;
    }

    const NUMBERS = /[0-9]/;
    if ((char && NUMBERS.test(char)) || char === "-" || char === ".") {
      let value = "";

      if (char === "-") {
        value += char;
        char = input[++current];
      }

      while ((char && NUMBERS.test(char)) || char === "." || char === "e" || char === "E") {
        value += char;
        char = input[++current];
      }

      // Handle case where e/E is followed by +/- sign
      if (char === "+" || char === "-") {
        const prevChar = value[value.length - 1];
        if (prevChar === "e" || prevChar === "E") {
          value += char;
          char = input[++current];
          while (char && NUMBERS.test(char)) {
            value += char;
            char = input[++current];
          }
        }
      }

      tokens.push({ type: "number", value });
      continue;
    }

    const LETTERS = /[a-z]/i;
    if (char && LETTERS.test(char)) {
      let value = "";

      while (char && LETTERS.test(char)) {
        if (current === input.length) {
          break;
        }
        value += char;

        char = input[++current];
      }

      if (value === "true" || value === "false" || value === "null") {
        tokens.push({ type: "name", value });
      } else {
        // Unknown token, such as `nul`, is not yet the complete form of `null`.
        current++;
        continue;
      }
      continue;
    }

    current++;
  }

  return tokens;
}

/**
 * Remove trailing tokens that may cause parsing failures.
 */
function strip(tokens: Token[]): Token[] {
  if (tokens.length === 0) {
    return tokens;
  }

  const lastToken = tokens[tokens.length - 1]!;

  switch (lastToken.type) {
    case "separator": {
      tokens = tokens.slice(0, tokens.length - 1);
      return strip(tokens);
    }
    case "number": {
      const lastCharacterOfLastToken = lastToken.value[lastToken.value.length - 1];
      if (lastCharacterOfLastToken === "." || lastCharacterOfLastToken === "-") {
        tokens = tokens.slice(0, tokens.length - 1);
        return strip(tokens);
      }
      break;
    }
    case "string": {
      const tokenBeforeTheLastToken = tokens[tokens.length - 2];
      if (tokenBeforeTheLastToken?.type === "delimiter") {
        tokens = tokens.slice(0, tokens.length - 1);
        return strip(tokens);
      } else if (
        tokenBeforeTheLastToken?.type === "brace" &&
        tokenBeforeTheLastToken.value === "{"
      ) {
        tokens = tokens.slice(0, tokens.length - 1);
        return strip(tokens);
      }
      break;
    }
    case "delimiter": {
      tokens = tokens.slice(0, tokens.length - 1);
      return strip(tokens);
    }
  }

  return tokens;
}

/**
 * Auto-completion of closing symbols
 */
function unstrip(tokens: Token[]): Token[] {
  const stack: string[] = [];

  for (const token of tokens) {
    if (token.type === "brace") {
      if (token.value === "{") {
        stack.push("}");
      } else {
        if (stack.length > 0 && stack[stack.length - 1] === "}") {
          stack.pop();
        }
      }
    }
    if (token.type === "paren") {
      if (token.value === "[") {
        stack.push("]");
      } else {
        if (stack.length > 0 && stack[stack.length - 1] === "]") {
          stack.pop();
        }
      }
    }
  }

  if (stack.length > 0) {
    stack.reverse().forEach(item => {
      if (item === "}") {
        tokens.push({ type: "brace", value: "}" });
      } else if (item === "]") {
        tokens.push({ type: "paren", value: "]" });
      }
    });
  }

  return tokens;
}

/**
 * Generate a JSON string from the token stream.
 */
function generate(tokens: Token[]): string {
  let output = "";

  for (const token of tokens) {
    switch (token.type) {
      case "string": {
        output += JSON.stringify(token.value);
        break;
      }
      default: {
        output += token.value;
        break;
      }
    }
  }

  return output;
}

/**
 * Parse a partial JSON string
 *
 * @param input: An incomplete JSON string
 * @returns: The parsed value; returns undefined if the string cannot be parsed
 */
export function partialParse(input: string): unknown {
  try {
    return JSON.parse(generate(unstrip(strip(tokenize(input)))));
  } catch {
    // If the parsing fails, return undefined.
    return undefined;
  }
}

/**
 * Check if the input is a valid partial JSON (can be parsed further)
 *
 * Performs tokenization and basic structural checks:
 * - Must not be empty or whitespace-only
 * - Must have balanced brackets (no more closing than opening at any point)
 * - Must have at least one structural token (brace/paren/separator/delimiter/name/number)
 * - A lone string token is rejected (likely an incomplete key name)
 *
 * @param input JSON string
 * @returns Whether it is valid
 */
export function isValidPartialJson(input: string): boolean {
  try {
    const trimmed = input.trim();
    if (!trimmed) {
      return false;
    }

    const tokens = tokenize(input);
    if (tokens.length === 0) {
      return false;
    }

    // If there is only one string token, it might be that the key name is incomplete.
    if (tokens.length === 1 && tokens[0]!.type === "string") {
      return false;
    }

    // Check bracket balance: never have more closing than opening at any point
    let depth = 0;
    let bracketDepth = 0;
    for (const token of tokens) {
      if (token.type === "brace") {
        if (token.value === "{") {
          depth++;
        } else {
          depth--;
          if (depth < 0) {
            return false; // Extra closing brace
          }
        }
      }
      if (token.type === "paren") {
        if (token.value === "[") {
          bracketDepth++;
        } else {
          bracketDepth--;
          if (bracketDepth < 0) {
            return false; // Extra closing bracket
          }
        }
      }
    }

    return true;
  } catch {
    return false;
  }
}
