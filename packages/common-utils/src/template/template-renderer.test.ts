import { describe, it, expect } from "vitest";
import {
  renderTemplate,
  renderTemplates,
  validateTemplateVariables,
  TemplateRenderError,
} from "./template-renderer.js";

describe("template-renderer", () => {
  describe("renderTemplate", () => {
    it("An empty string should be returned when the template is empty.", () => {
      expect(renderTemplate("", {})).toBe("");
    });

    it("The original template should be returned when the variable is empty.", () => {
      expect(renderTemplate("Hello {{name}}", {})).toBe("Hello {{name}}");
    });

    it("The original template should be returned when the variable is null.", () => {
      expect(renderTemplate("Hello {{name}}", null as any)).toBe("Hello {{name}}");
    });

    it("The original template should be returned when the variable is undefined.", () => {
      expect(renderTemplate("Hello {{name}}", undefined as any)).toBe("Hello {{name}}");
    });

    it("The individual variables should be replaced correctly.", () => {
      const result = renderTemplate("Hello, {{name}}!", { name: "Alice" });
      expect(result).toBe("Hello, Alice!");
    });

    it("The multiple variables should be replaced correctly.", () => {
      const result = renderTemplate("Hello, {{name}}! Today is {{date}}.", {
        name: "Alice",
        date: "2024-01-01",
      });
      expect(result).toBe("Hello, Alice! Today is 2024-01-01.");
    });

    it("The nested variables should be handled correctly.", () => {
      const result = renderTemplate("User: {{user.name}}, Age: {{user.age}}", {
        user: { name: "Bob", age: 30 },
      });
      expect(result).toBe("User: Bob, Age: 30");
    });

    it("The array indices should be handled correctly.", () => {
      const result = renderTemplate("First item: {{items[0].name}}", {
        items: [{ name: "Item 1" }, { name: "Item 2" }],
      });
      expect(result).toBe("First item: Item 1");
    });

    it("The placeholder for the undefined variable should be retained.", () => {
      const result = renderTemplate("Hello, {{name}}! {{missing}}", { name: "Alice" });
      expect(result).toBe("Hello, Alice! {{missing}}");
    });

    it("The null variable placeholder should be retained.", () => {
      const result = renderTemplate("Hello, {{name}}! {{nullVar}}", {
        name: "Alice",
        nullVar: null,
      });
      expect(result).toBe("Hello, Alice! {{nullVar}}");
    });

    it("The whitespace characters around the variable names should be processed.", () => {
      const result = renderTemplate("Hello, {{ name }}!", { name: "Alice" });
      expect(result).toBe("Hello, Alice!");
    });

    it("The numbers should be converted to strings.", () => {
      const result = renderTemplate("Count: {{count}}", { count: 42 });
      expect(result).toBe("Count: 42");
    });

    it("Boolean values should be converted to strings.", () => {
      const result = renderTemplate("Active: {{active}}", { active: true });
      expect(result).toBe("Active: true");
    });

    it("The object should be converted to a string.", () => {
      const result = renderTemplate("Data: {{data}}", { data: { key: "value" } });
      expect(result).toBe("Data: [object Object]");
    });

    it("Consecutive placeholders should be processed.", () => {
      const result = renderTemplate("{{first}}{{second}}{{third}}", {
        first: "A",
        second: "B",
        third: "C",
      });
      expect(result).toBe("ABC");
    });

    it("Duplicate variables should be handled accordingly.", () => {
      const result = renderTemplate("{{name}} says hello to {{name}}", { name: "Alice" });
      expect(result).toBe("Alice says hello to Alice");
    });

    it("The value of the empty string variable should be handled accordingly.", () => {
      const result = renderTemplate("Hello, {{name}}!", { name: "" });
      expect(result).toBe("Hello, !");
    });

    it("The value 0 should be handled accordingly.", () => {
      const result = renderTemplate("Count: {{count}}", { count: 0 });
      expect(result).toBe("Count: 0");
    });

    it("The false value should be handled accordingly.", () => {
      const result = renderTemplate("Active: {{active}}", { active: false });
      expect(result).toBe("Active: false");
    });

    it("Text to translate: Text that does not contain curly braces should be processed.", () => {
      const result = renderTemplate("Hello, World!", { name: "Alice" });
      expect(result).toBe("Hello, World!");
    });

    it("We should handle the situation where there are only curly braces but no variable names.", () => {
      const result = renderTemplate("Hello, {{}}!", { name: "Alice" });
      expect(result).toBe("Hello, {{}}!");
    });
  });

  describe("Conditional rendering {{#if}}", () => {
    it("The content that should be rendered when the condition is true.", () => {
      const result = renderTemplate("{{#if showName}}Name: {{name}}{{/if}}", {
        showName: true,
        name: "Alice",
      });
      expect(result).toBe("Name: Alice");
    });

    it("The content that should be hidden when the condition is false should be obscured.", () => {
      const result = renderTemplate("{{#if showName}}Name: {{name}}{{/if}}", {
        showName: false,
        name: "Alice",
      });
      expect(result).toBe("");
    });

    it("Non-empty strings should be considered as true values.", () => {
      const result = renderTemplate("{{#if name}}Has name{{/if}}", { name: "Alice" });
      expect(result).toBe("Has name");
    });

    it("Empty strings should be considered as false values.", () => {
      const result = renderTemplate("{{#if name}}Has name{{/if}}", { name: "" });
      expect(result).toBe("");
    });

    it("Non-zero numbers should be considered as true values.", () => {
      const result = renderTemplate("{{#if count}}Has count{{/if}}", { count: 5 });
      expect(result).toBe("Has count");
    });

    it("Zero should be considered a false value.", () => {
      const result = renderTemplate("{{#if count}}Has count{{/if}}", { count: 0 });
      expect(result).toBe("");
    });

    it("Non-empty arrays should be considered as true values.", () => {
      const result = renderTemplate("{{#if items}}Has items{{/if}}", { items: [1, 2, 3] });
      expect(result).toBe("Has items");
    });

    it("Empty arrays should be considered as false values.", () => {
      const result = renderTemplate("{{#if items}}Has items{{/if}}", { items: [] });
      expect(result).toBe("");
    });

    it("`null should be treated as a false value`", () => {
      const result = renderTemplate("{{#if value}}Has value{{/if}}", { value: null });
      expect(result).toBe("");
    });

    it("`undefined should be treated as a false value.`", () => {
      const result = renderTemplate("{{#if value}}Has value{{/if}}", { value: undefined });
      expect(result).toBe("");
    });

    it("Nested attributes should be supported as conditions.", () => {
      const result = renderTemplate("{{#if user.active}}Active user{{/if}}", {
        user: { active: true },
      });
      expect(result).toBe("Active user");
    });

    it("Multi-line conditional content should be handled accordingly.", () => {
      const template = `{{#if showDetails}}
Name: {{name}}
Age: {{age}}
{{/if}}`;
      const result = renderTemplate(template, { showDetails: true, name: "Alice", age: 30 });
      expect(result).toBe(`
Name: Alice
Age: 30
`);
    });

    it("Multiple condition blocks should be processed.", () => {
      const template = "{{#if a}}A{{/if}}{{#if b}}B{{/if}}{{#if c}}C{{/if}}";
      const result = renderTemplate(template, { a: true, b: false, c: true });
      expect(result).toBe("AC");
    });
  });

  describe("Loop rendering with {{#each}}", () => {
    it("Each element in the array should be rendered.", () => {
      const result = renderTemplate("Items:{{#each items}} {{this}}{{/each}}", {
        items: ["A", "B", "C"],
      });
      expect(result).toBe("Items: A B C");
    });

    it("An empty array should be rendered as an empty string.", () => {
      const result = renderTemplate("Items:{{#each items}} {{this}}{{/each}}", { items: [] });
      expect(result).toBe("Items:");
    });

    it("The non-array should be rendered as an empty string.", () => {
      const result = renderTemplate("Items:{{#each items}} {{this}}{{/each}}", {
        items: "not array",
      });
      expect(result).toBe("Items:");
    });

    it("Access to object array properties should be supported.", () => {
      const result = renderTemplate("{{#each users}}- {{this.name}} ({{this.age}})\n{{/each}}", {
        users: [
          { name: "Alice", age: 30 },
          { name: "Bob", age: 25 },
        ],
      });
      expect(result).toBe("- Alice (30)\n- Bob (25)\n");
    });

    it("The @index variable should be supported.", () => {
      const result = renderTemplate("{{#each items}}{{@index}}: {{this}}\n{{/each}}", {
        items: ["A", "B", "C"],
      });
      expect(result).toBe("0: A\n1: B\n2: C\n");
    });

    it("The @first variable should be supported.", () => {
      const result = renderTemplate(
        "{{#each items}}{{#if @first}}[FIRST] {{/if}}{{this}}\n{{/each}}",
        {
          items: ["A", "B", "C"],
        },
      );
      expect(result).toBe("[FIRST] A\nB\nC\n");
    });

    it("The @last variable should be supported.", () => {
      const result = renderTemplate(
        "{{#each items}}{{this}}{{#if @last}}[LAST]{{/if}}\n{{/each}}",
        {
          items: ["A", "B", "C"],
        },
      );
      expect(result).toBe("A\nB\nC[LAST]\n");
    });

    it("Multi-line loop content should be handled accordingly.", () => {
      const template = `Items:
{{#each items}}
  - {{this}}
{{/each}}`;
      const result = renderTemplate(template, { items: ["A", "B"] });
      expect(result).toBe(`Items:

  - A

  - B
`);
    });

    it("Nested path access to arrays should be supported.", () => {
      const result = renderTemplate("{{#each data.items}}{{this}}\n{{/each}}", {
        data: { items: ["X", "Y", "Z"] },
      });
      expect(result).toBe("X\nY\nZ\n");
    });

    it("The digital array should be processed.", () => {
      const result = renderTemplate("Sum: {{#each nums}}{{this}}+{{/each}}", { nums: [1, 2, 3] });
      expect(result).toBe("Sum: 1+2+3+");
    });

    it("Arrays of strings containing special characters should be processed accordingly.", () => {
      const result = renderTemplate("{{#each items}}{{this}}{{/each}}", {
        items: ["hello", "world", "!"],
      });
      expect(result).toBe("helloworld!");
    });
  });

  describe("Combination of Conditions and Loops", () => {
    it("Conditional nested loops should be supported.", () => {
      const template = "{{#if showItems}}{{#each items}}{{this}} {{/each}}{{/if}}";
      const result = renderTemplate(template, { showItems: true, items: ["A", "B"] });
      expect(result).toBe("A B ");
    });

    it("Looping with nested conditions should be supported.", () => {
      const template = "{{#each items}}{{#if this.active}}{{this.name}} {{/if}}{{/each}}";
      const result = renderTemplate(template, {
        items: [
          { name: "A", active: true },
          { name: "B", active: false },
          { name: "C", active: true },
        ],
      });
      expect(result).toBe("A C ");
    });

    it("Complex nested structures need to be handled accordingly.", () => {
      const template = `{{#if hasData}}
Data:
{{#each items}}
  {{@index}}. {{this.name}}
  {{#if this.active}}(active){{/if}}
{{/each}}
{{/if}}`;
      const result = renderTemplate(template, {
        hasData: true,
        items: [
          { name: "A", active: true },
          { name: "B", active: false },
        ],
      });
      // Note: Each item within the loop will retain its newline character, even if the condition is false.
      // There is a newline character before the {{/if}} at the end of the template, so there is also a newline character at the end of the result.
      expect(result).toBe(`
Data:

  0. A
  (active)

  1. B
  

`);
    });
  });

  describe("renderTemplates", () => {
    it("Multiple templates should be rendered in batches.", () => {
      const templates = ["Hello, {{name}}!", "Goodbye, {{name}}!", "See you, {{name}}!"];
      const result = renderTemplates(templates, { name: "Alice" });
      expect(result).toEqual(["Hello, Alice!", "Goodbye, Alice!", "See you, Alice!"]);
    });

    it("An empty array should be handled accordingly.", () => {
      const result = renderTemplates([], { name: "Alice" });
      expect(result).toEqual([]);
    });

    it("Mixed templates should be processed accordingly.", () => {
      const templates = ["Hello, {{name}}!", "Just text", "Missing {{var}}"];
      const result = renderTemplates(templates, { name: "Alice" });
      expect(result).toEqual(["Hello, Alice!", "Just text", "Missing {{var}}"]);
    });
  });

  describe("validateTemplateVariables", () => {
    it("An empty array should be returned when all variables exist.", () => {
      const missing = validateTemplateVariables("Hello, {{name}}!", { name: "Alice" });
      expect(missing).toEqual([]);
    });

    it("The missing variable names should be returned.", () => {
      const missing = validateTemplateVariables("Hello, {{name}}! {{missing}}", { name: "Alice" });
      expect(missing).toEqual(["missing"]);
    });

    it("Multiple missing variable names should be returned.", () => {
      const missing = validateTemplateVariables("{{var1}} and {{var2}} and {{var3}}", {
        var1: "value1",
      });
      expect(missing).toEqual(["var2", "var3"]);
    });

    it("Nested variables should be detected.", () => {
      const missing = validateTemplateVariables("{{user.name}}", { user: { age: 30 } });
      expect(missing).toEqual(["user.name"]);
    });

    it("The array index variable should be checked.", () => {
      const missing = validateTemplateVariables("{{items[0].name}}", { items: [] });
      expect(missing).toEqual(["items[0].name"]);
    });

    it("Null value variables should be handled accordingly.", () => {
      const missing = validateTemplateVariables("{{name}} and {{nullVar}}", {
        name: "Alice",
        nullVar: null,
      });
      expect(missing).toEqual(["nullVar"]);
    });

    it("The `undefined` value variables should be handled accordingly.", () => {
      const missing = validateTemplateVariables("{{name}} and {{undefinedVar}}", {
        name: "Alice",
        undefinedVar: undefined,
      });
      expect(missing).toEqual(["undefinedVar"]);
    });

    it("Duplicate variables should be handled accordingly.", () => {
      const missing = validateTemplateVariables("{{name}} and {{name}}", {});
      expect(missing).toEqual(["name"]);
    });

    it("The empty template should be processed.", () => {
      const missing = validateTemplateVariables("", { name: "Alice" });
      expect(missing).toEqual([]);
    });

    it("The template that does not contain any placeholders should be processed.", () => {
      const missing = validateTemplateVariables("Just plain text", { name: "Alice" });
      expect(missing).toEqual([]);
    });

    it("The whitespace characters around the variable names should be processed.", () => {
      const missing = validateTemplateVariables("{{ name }} and {{ missing }}", { name: "Alice" });
      expect(missing).toEqual(["missing"]);
    });

    it("The 0-value variable should be handled accordingly.", () => {
      const missing = validateTemplateVariables("{{count}}", { count: 0 });
      expect(missing).toEqual([]);
    });

    it("False value variables should be handled accordingly.", () => {
      const missing = validateTemplateVariables("{{active}}", { active: false });
      expect(missing).toEqual([]);
    });

    it("The empty string variable should be handled accordingly.", () => {
      const missing = validateTemplateVariables("{{name}}", { name: "" });
      expect(missing).toEqual([]);
    });
  });

  describe("Error message", () => {
    describe("Error in looping the special variable", () => {
      it("An error should be thrown when using @index outside of a loop.", () => {
        expect(() => renderTemplate("{{@index}}", {})).toThrow(TemplateRenderError);
        expect(() => renderTemplate("{{@index}}", {})).toThrow(/只能在.*循环内部使用/);
      });

      it("An error should be thrown when using @first outside of a loop.", () => {
        expect(() => renderTemplate("{{@first}}", {})).toThrow(TemplateRenderError);
        expect(() => renderTemplate("{{@first}}", {})).toThrow(/只能在.*循环内部使用/);
      });

      it("An error should be thrown when using @last outside of a loop.", () => {
        expect(() => renderTemplate("{{@last}}", {})).toThrow(TemplateRenderError);
        expect(() => renderTemplate("{{@last}}", {})).toThrow(/只能在.*循环内部使用/);
      });

      it("An error should be thrown when special loop variables that are not supported are encountered.", () => {
        expect(() =>
          renderTemplate("{{#each items}}{{@invalid}}{{/each}}", { items: [1, 2] }),
        ).toThrow(TemplateRenderError);
        expect(() =>
          renderTemplate("{{#each items}}{{@invalid}}{{/each}}", { items: [1, 2] }),
        ).toThrow(/不支持的循环特殊变量/);
      });

      it("An error should be thrown when using a special loop variable that is not supported within the condition.", () => {
        expect(() =>
          renderTemplate("{{#each items}}{{#if @invalid}}content{{/if}}{{/each}}", {
            items: [1, 2],
          }),
        ).toThrow(TemplateRenderError);
        expect(() =>
          renderTemplate("{{#each items}}{{#if @invalid}}content{{/if}}{{/each}}", {
            items: [1, 2],
          }),
        ).toThrow(/不支持的循环特殊变量/);
      });
    });

    describe("This variable is incorrect.", () => {
      it("An error should be thrown when using `this` outside of a loop.", () => {
        expect(() => renderTemplate("{{this}}", {})).toThrow(TemplateRenderError);
        expect(() => renderTemplate("{{this}}", {})).toThrow(/只能在.*循环内部使用/);
      });

      it("An error should be thrown when using this.property outside of the loop.", () => {
        expect(() => renderTemplate("{{this.name}}", {})).toThrow(TemplateRenderError);
        expect(() => renderTemplate("{{this.name}}", {})).toThrow(/只能在.*循环内部使用/);
      });

      it("An error should be thrown when using `this` in a condition outside of the loop.", () => {
        expect(() => renderTemplate("{{#if this.active}}content{{/if}}", {})).toThrow(
          TemplateRenderError,
        );
        expect(() => renderTemplate("{{#if this.active}}content{{/if}}", {})).toThrow(
          /只能在.*循环内部使用/,
        );
      });
    });

    describe("Error message content", () => {
      it("The error message should include the variable names.", () => {
        try {
          renderTemplate("{{@invalidVar}}", {});
        } catch (e) {
          expect(e).toBeInstanceOf(TemplateRenderError);
          const error = e as TemplateRenderError;
          expect(error.variableName).toBe("@invalidVar");
        }
      });

      it("The error message should include contextual information.", () => {
        try {
          renderTemplate("{{@index}}", {});
        } catch (e) {
          expect(e).toBeInstanceOf(TemplateRenderError);
          const error = e as TemplateRenderError;
          expect(error.context).toBe("variable");
        }
      });

      it("The correct context should be included in the error message when a condition is incorrect.", () => {
        try {
          renderTemplate("{{#if @index}}content{{/if}}", {});
        } catch (e) {
          expect(e).toBeInstanceOf(TemplateRenderError);
          const error = e as TemplateRenderError;
          expect(error.context).toBe("{{#if}}");
        }
      });
    });
  });
});
