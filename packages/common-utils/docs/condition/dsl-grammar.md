# Condition Expression DSL Grammar Specification (Chevrotain Implementation)

## Overview

This document defines the formal grammar for the Condition Expression DSL using Chevrotain's parsing framework. The DSL is used for workflow condition evaluation, supporting comparisons, logical operations, array methods, function calls, and more.

**Note**: This specification describes the grammar in Chevrotain's rule-based notation rather than traditional PEG. The grammar structure follows the same precedence rules but uses Chevrotain's `RULE`, `SUBRULE`, `CONSUME`, `OR`, `MANY`, and `OPTION` combinators.

## Operator Precedence (Lowest to Highest)

| Precedence | Category    | Associativity | Operators                           |
|------------|-------------|---------------|-------------------------------------|
| 1          | Ternary     | Right         | `? :`                               |
| 2          | Logical OR  | Left          | `\|\|`                              |
| 3          | Logical AND | Left          | `&&`                                |
| 4          | NOT         | Right         | `!`                                 |
| 5          | Comparison  | Left          | `==` `!=` `>=` `<=` `>` `<` `contains` `in` |
| 6          | Add/Sub     | Left          | `+` `-`                             |
| 7          | Mul/Div/Mod | Left          | `*` `/` `%`                         |
| 8          | Unary       | Right         | `-` (negative)                      |
| 9          | Primary     | -             | member access, call, literal, group |

## Chevrotain Grammar Rules

### Top-Level Rule

```typescript
// Entry point: expression is the top-level rule
expression = this.RULE("expression", () => {
  this.SUBRULE(this.ternary);
});
```

### Ternary Operator (Right-Associative)

The ternary operator is right-associative: `a ? b : c ? d : e` parses as `a ? b : (c ? d : e)`.

```typescript
private ternary = this.RULE("ternary", () => {
  this.OR([
    { ALT: () => {
        // Ternary form: condition ? consequent : alternate
        this.SUBRULE(this.logicalOr);           // condition
        this.CONSUME(Ternary);                  // ?
        this.SUBRULE(this.ternary, { LABEL: "consequent" });  // consequent
        this.CONSUME(Colon);                    // :
        this.SUBRULE(this.ternary, { LABEL: "alternate" });   // alternate
      }
    },
    { ALT: () => this.SUBRULE(this.logicalOr) }  // Non-ternary form
  ]);
});
```

**CST Structure:**
```typescript
interface TernaryCstNode {
  name: "ternary";
  children: {
    logicalAnd: CstNode[];           // condition (at least one)
    Ternary?: IToken[];              // ? token
    ternary_consequent: CstNode[];   // consequent expressions
    Colon?: IToken[];                // : token
    ternary_alternate: CstNode[];    // alternate expressions
  };
}
```

### Logical OR (Left-Associative)

Logical OR uses `MANY()` to collect multiple operands, which are then folded into a left-leaning tree during visitor conversion.

```typescript
private logicalOr = this.RULE("logicalOr", () => {
  this.SUBRULE(this.logicalAnd);      // First operand (left)
  this.MANY(() => {
    this.CONSUME(LogicalOr);          // || token
    this.SUBRULE2(this.logicalAnd);   // Subsequent operands (right)
  });
});
```

**CST Structure:**
```typescript
interface LogicalOrCstNode {
  name: "logicalOr";
  children: {
    logicalAnd: CstNode[];   // [left, right1, right2, ...] - flat array
    LogicalOr: IToken[];     // [||, ||, ...] - one per operation
  };
}
```

**Visitor Conversion (Left-Folding):**
```typescript
visitLogicalOr(ctx: any): Expression {
  let left = this.visit(ctx.logicalAnd[0]);
  for (let i = 1; i < ctx.logicalAnd.length; i++) {
    left = {
      type: "binary",
      operator: "||",
      left: left,
      right: this.visit(ctx.logicalAnd[i])
    };
  }
  return left;
}
```

### Logical AND (Left-Associative)

Same pattern as logical OR.

```typescript
private logicalAnd = this.RULE("logicalAnd", () => {
  this.SUBRULE(this.notExpr);       // First operand
  this.MANY(() => {
    this.CONSUME(LogicalAnd);       // && token
    this.SUBRULE2(this.notExpr);    // Subsequent operands
  });
});
```

### NOT Operator (Right-Associative)

NOT is right-associative: `!!x` parses as `!( !x )`.

```typescript
private notExpr = this.RULE("notExpr", () => {
  this.OR([
    { ALT: () => { 
        this.CONSUME(Not);          // ! token
        this.SUBRULE(this.notExpr); // Nested NOT
      }
    },
    { ALT: () => this.SUBRULE(this.comparison) }  // Base case
  ]);
});
```

### Comparison Operators (Left-Associative)

Comparison operators include `==`, `!=`, `>=`, `<=`, `>`, `<`, `contains`, and `in`.

```typescript
private comparison = this.RULE("comparison", () => {
  this.SUBRULE(this.addition);      // Left operand
  this.OPTION(() => {
    this.CONSUME(ComparisonOp);     // Optional operator
    this.SUBRULE2(this.addition);   // Right operand
  });
});
```

**CST Structure:**
```typescript
interface ComparisonCstNode {
  name: "comparison";
  children: {
    addition: CstNode[];            // Left operand
    ComparisonOp?: IToken[];        // Optional operator
    addition?: CstNode[];           // Optional right operand
  };
}
```

**Visitor Conversion:**
```typescript
visitComparison(ctx: any): Expression {
  const left = this.visit(ctx.addition[0]);
  if (!ctx.ComparisonOp || ctx.ComparisonOp.length === 0) {
    return left;  // No operator, just return the operand
  }
  const operator = ctx.ComparisonOp[0].image;
  const right = this.visit(ctx.addition[1]);
  return {
    type: "binary",
    operator,
    left,
    right
  };
}
```

### Addition and Subtraction (Left-Associative)

```typescript
private addition = this.RULE("addition", () => {
  this.SUBRULE(this.multiplication);  // Left operand
  this.MANY(() => {
    this.OR([
      { ALT: () => this.CONSUME(Plus) },   // +
      { ALT: () => this.CONSUME(Minus) }   // -
    ]);
    this.SUBRULE2(this.multiplication);   // Right operand
  });
});
```

### Multiplication, Division, Modulo (Left-Associative)

```typescript
private multiplication = this.RULE("multiplication", () => {
  this.SUBRULE(this.unary);             // Left operand
  this.MANY(() => {
    this.OR([
      { ALT: () => this.CONSUME(Multiply) },  // *
      { ALT: () => this.CONSUME(Divide) },    // /
      { ALT: () => this.CONSUME(Modulo) }    // %
    ]);
    this.SUBRULE2(this.unary);              // Right operand
  });
});
```

### Unary Minus (Right-Associative)

Unary minus is explicitly handled as a separate node type (`unaryMinus`) instead of the old `0 - x` hack.

```typescript
private unary = this.RULE("unary", () => {
  this.OR([
    { ALT: () => { 
        this.CONSUME(Minus);          // - token
        this.SUBRULE(this.unary);     // Nested unary (for --x, etc.)
      }
    },
    { ALT: () => this.SUBRULE(this.primary) }  // Base case
  ]);
});
```

**Visitor Conversion:**
```typescript
visitUnary(ctx: any): Expression {
  if (ctx.Minus && ctx.Minus.length > 0) {
    const operand = this.visit(ctx.unary[0]);
    return {
      type: "unaryMinus",
      operand,
      metadata: this.extractMetadata(ctx)
    };
  }
  return this.visit(ctx.primary[0]);
}
```

### Primary Expressions

Primary expressions are the base cases that cannot be decomposed further.

```typescript
private primary = this.RULE("primary", () => {
  this.OR([
    { ALT: () => this.SUBRULE(this.functionCall) },   // Function calls
    { ALT: () => this.SUBRULE(this.arrayLiteral) },   // Array literals [1, 2, 3]
    { ALT: () => this.CONSUME(StringLiteral) },       // String literals
    { ALT: () => this.CONSUME(NumberLiteral) },       // Number literals
    { ALT: () => this.CONSUME(True) },                // true
    { ALT: () => this.CONSUME(False) },               // false
    { ALT: () => this.CONSUME(Null) },                // null
    { ALT: () => this.SUBRULE(this.memberAccess) },   // Property access chains
    { ALT: () => {                                   // Parenthesized expressions
        this.CONSUME(LParen);
        this.SUBRULE(this.expression);
        this.CONSUME(RParen);
      }
    }
  ]);
});
```

### Member Access Chain

Member access supports chained property access and method calls: `obj.prop1.prop2.method(args)`.

```typescript
private memberAccess = this.RULE("memberAccess", () => {
  this.SUBRULE(this.primary);       // Start with a primary (usually identifier)
  this.MANY(() => {
    this.CONSUME(Dot);              // . token
    this.OR([
      { ALT: () => this.SUBRULE(this.methodCall) },  // Method call
      { ALT: () => this.CONSUME(Identifier, { LABEL: "property" }) }  // Property access
    ]);
  });
});
```

**CST Structure:**
```typescript
interface MemberAccessCstNode {
  name: "memberAccess";
  children: {
    primary: CstNode[];              // Leftmost primary (e.g., "obj")
    Dot: IToken[];                   // Multiple . tokens
    Identifier?: CstNode[];          // Property names (if any)
    methodCall?: CstNode[];          // Method calls (if any)
  };
}
```

**Visitor Conversion:**
```typescript
visitMemberAccess(ctx: any): Expression {
  let obj = this.visit(ctx.primary[0]);  // Start with leftmost primary
  
  // Process each suffix (.prop or .method())
  for (let i = 0; i < ctx.Dot.length; i++) {
    if (ctx.Identifier && ctx.Identifier[i + 1]) {
      // Property access: obj.prop
      const propName = ctx.Identifier[i + 1][0].image;
      obj = {
        type: "memberAccess",
        object: obj,
        property: propName
      };
    } else if (ctx.methodCall && ctx.methodCall[i]) {
      // Method call: obj.method()
      const methodCallCtx = ctx.methodCall[i];
      const methodName = this.extractMethodName(methodCallCtx);
      const args = this.visit(methodCallCtx.children.argumentList);
      
      obj = {
        type: "call",
        callee: {
          type: "memberAccess",
          object: obj,
          property: methodName
        },
        arguments: args,
        methodKind: this.determineMethodKind(methodName)
      };
    }
  }
  
  return obj;
}
```

### Method Call (Array/String Methods)

Method calls are restricted to predefined array and string methods.

```typescript
private methodCall = this.RULE("methodCall", () => {
  this.OR([
    { ALT: () => this.CONSUME(ArrayMethod) },  // Array method name
    { ALT: () => this.CONSUME(StringMethod) }  // String method name
  ]);
  this.CONSUME(LParen);
  this.SUBRULE(this.argumentList);
  this.CONSUME(RParen);
});
```

**Visitor Conversion:**
```typescript
visitMethodCall(ctx: any): CallExpr {
  const methodName = this.extractMethodName(ctx);
  const args = this.visit(ctx.argumentList);
  
  return {
    type: "call",
    callee: { /* will be set by parent memberAccess */ },
    arguments: args,
    methodKind: this.determineMethodKind(methodName)
  };
}

private extractMethodName(ctx: any): string {
  if (ctx.ArrayMethod && ctx.ArrayMethod.length > 0) {
    return ctx.ArrayMethod[0].image;
  }
  if (ctx.StringMethod && ctx.StringMethod.length > 0) {
    return ctx.StringMethod[0].image;
  }
  throw new Error("Unknown method type");
}

private determineMethodKind(methodName: string): "arrayMethod" | "stringMethod" {
  const arrayMethods = [
    "someEqual", "someContains", "everyEqual", "everyHas", 
    "countWhere", "countWhereContains", "findEqual", "findContains",
    "has", "hasContains", "sum", "avg", "min", "max",
    "someGreaterThan", "someLessThan", "everyGreaterThan", "everyLessThan",
    "map", "distinct", "first", "last"
  ];
  
  if (arrayMethods.includes(methodName)) {
    return "arrayMethod";
  }
  
  const stringMethods = ["startsWith", "endsWith", "length", "toLowerCase", "toUpperCase", "trim"];
  if (stringMethods.includes(methodName)) {
    return "stringMethod";
  }
  
  throw new Error(`Unknown method: ${methodName}`);
}
```

### Function Call

Function calls can use any identifier as the function name.

```typescript
private functionCall = this.RULE("functionCall", () => {
  this.CONSUME(Identifier);       // Function name
  this.CONSUME(LParen);
  this.SUBRULE(this.argumentList);
  this.CONSUME(RParen);
});
```

**Visitor Conversion:**
```typescript
visitFunctionCall(ctx: any): CallExpr {
  const functionName = ctx.Identifier[0].image;
  const args = this.visit(ctx.argumentList);
  
  return {
    type: "call",
    callee: {
      type: "identifier",
      name: functionName
    },
    arguments: args
  };
}
```

### Array Literal

Array literals support optional elements: `[]` or `[1, 2, 3]`.

```typescript
private arrayLiteral = this.RULE("arrayLiteral", () => {
  this.CONSUME(LBracket);
  this.OPTION(() => this.SUBRULE(this.valueList));
  this.CONSUME(RBracket);
});
```

**Visitor Conversion:**
```typescript
visitArrayLiteral(ctx: any): ArrayLiteralExpr {
  const elements = ctx.valueList 
    ? this.visit(ctx.valueList[0]).elements 
    : [];
  
  return {
    type: "arrayLiteral",
    elements,
    metadata: this.extractMetadata(ctx)
  };
}
```

### Value List (for Arrays)

Value lists contain comma-separated literals.

```typescript
private valueList = this.RULE("valueList", () => {
  this.SUBRULE(this.literal);       // First element
  this.MANY(() => {
    this.CONSUME(Comma);            // , token
    this.SUBRULE2(this.literal);    // Subsequent elements
  });
});
```

**Visitor Conversion:**
```typescript
visitValueList(ctx: any): ArrayLiteralExpr {
  const elements = ctx.literal.map(litCtx => this.visit(litCtx[0]));
  return {
    type: "arrayLiteral",
    elements,
    metadata: this.extractMetadata(ctx)
  };
}
```

### Argument List (for Calls)

Argument lists support optional arguments: `func()` or `func(a, b, c)`.

```typescript
private argumentList = this.RULE("argumentList", () => {
  this.OPTION(() => {
    this.SUBRULE(this.expression);        // First argument
    this.MANY(() => {
      this.CONSUME(Comma);                // , token
      this.SUBRULE2(this.expression);     // Subsequent arguments
    });
  });
});
```

**Visitor Conversion:**
```typescript
visitArgumentList(ctx: any): Expression[] {
  if (!ctx.expression || ctx.expression.length === 0) {
    return [];
  }
  
  const args = [this.visit(ctx.expression[0])];
  for (let i = 1; i < ctx.expression.length; i++) {
    args.push(this.visit(ctx.expression[i]));
  }
  return args;
}
```

### Literals

Literals include strings, numbers, booleans, and null.

```typescript
private literal = this.RULE("literal", () => {
  this.OR([
    { ALT: () => this.CONSUME(StringLiteral) },
    { ALT: () => this.CONSUME(NumberLiteral) },
    { ALT: () => this.CONSUME(True) },
    { ALT: () => this.CONSUME(False) },
    { ALT: () => this.CONSUME(Null) }
  ]);
});
```

**Visitor Conversion:**
```typescript
visitLiteral(ctx: any): LiteralExpr {
  const token = ctx.StringLiteral?.[0] || 
                ctx.NumberLiteral?.[0] || 
                ctx.True?.[0] || 
                ctx.False?.[0] || 
                ctx.Null?.[0];
  
  if (!token) {
    throw new Error("No literal token found");
  }
  
  if (ctx.StringLiteral) {
    return {
      type: "literal",
      valueType: "string",
      value: this.unescapeString(token.image),
      metadata: this.extractMetadata(ctx)
    };
  }
  
  if (ctx.NumberLiteral) {
    return {
      type: "literal",
      valueType: "number",
      value: parseFloat(token.image),
      metadata: this.extractMetadata(ctx)
    };
  }
  
  if (ctx.True) {
    return {
      type: "literal",
      valueType: "boolean",
      value: true,
      metadata: this.extractMetadata(ctx)
    };
  }
  
  if (ctx.False) {
    return {
      type: "literal",
      valueType: "boolean",
      value: false,
      metadata: this.extractMetadata(ctx)
    };
  }
  
  if (ctx.Null) {
    return {
      type: "literal",
      valueType: "null",
      value: null,
 metadata: this.extractMetadata(ctx)
    };
  }
  
  throw new Error("Unknown literal type");
}

private unescapeString(image: string): string {
  // Remove surrounding quotes
  const content = image.slice(1, -1);
  // Unescape common sequences
  return content
    .replace(/\\'/g, "'")
    .replace(/\\"/g, '"')
    .replace(/\\\\/g, "\\")
    .replace(/\\n/g, "\n")
    .replace(/\\t/g, "\t")
    .replace(/\\r/g, "\r");
}
```

## AST Node Type Mapping

The grammar maps to the following unified AST node types:

| Grammar Rule          | AST Node Type      | Description                  |
|-----------------------|--------------------|------------------------------|
| `True` / `False`      | `literal`          | Boolean literal              |
| `NumberLiteral`       | `literal`          | Numeric literal              |
| `StringLiteral`       | `literal`          | String literal               |
| `Null`                | `literal`          | Null literal                 |
| `Identifier`          | `identifier`       | Variable/identifier          |
| `comparison`          | `binary`           | Comparison operation         |
| `logicalOr`           | `binary`           | Logical OR (`||`)            |
| `logicalAnd`          | `binary`           | Logical AND (`&&`)           |
| `notExpr`             | `not`              | NOT operation                |
| `addition`            | `binary`           | Arithmetic (+, -)            |
| `multiplication`      | `binary`           | Arithmetic (*, /, %)         |
| `unary` (with `-`)    | `unaryMinus`       | Unary minus                  |
| `methodCall`          | `call`             | Array/string method call     |
| `functionCall`        | `call`             | Custom function invocation   |
| `memberAccess`        | `memberAccess`     | Property access chain        |
| `ternary`             | `ternary`          | Ternary conditional          |
| `arrayLiteral`        | `arrayLiteral`     | Array literal                |

## Expression Examples

### Basic Comparisons

```typescript
// Numeric comparison
user.age >= 18
// AST: {
//   type: "binary",
//   operator: ">=",
//   left: { type: "memberAccess", object: {...}, property: "age" },
//   right: { type: "literal", valueType: "number", value: 18 }
// }

status == "active"
// AST: {
//   type: "binary",
//   operator: "==",
//   left: { type: "identifier", name: "status" },
//   right: { type: "literal", valueType: "string", value: "active" }
// }

score != 0
```

### Logical Combinations

```typescript
// AND/OR
age >= 18 && age <= 65
// AST: {
//   type: "binary",
//   operator: "&&",
//   left: { type: "binary", operator: ">=", ... },
//   right: { type: "binary", operator: "<=", ... }
// }

status == "active" || status == "pending"

// With NOT
!isDeleted
// AST: {
//   type: "not",
//   operand: { type: "identifier", name: "isDeleted" }
// }

!(age < 18)
```

### Arithmetic

```typescript
// Basic arithmetic
price * 0.9
// AST: {
//   type: "binary",
//   operator: "*",
//   left: { type: "identifier", name: "price" },
//   right: { type: "literal", valueType: "number", value: 0.9 }
// }

count + 1
total / items.length
value % 2 == 0

// Unary minus
-price
// AST: {
//   type: "unaryMinus",
//   operand: { type: "identifier", name: "price" }
// }

--counter
```

### String Methods

```typescript
name.startsWith("J")
// AST: {
//   type: "call",
//   callee: {
//     type: "memberAccess",
//  object: { type: "identifier", name: "name" },
//     property: "startsWith"
//   },
//   arguments: [
//     { type: "literal", valueType: "string", value: "J" }
//   ],
//   methodKind: "stringMethod"
// }

email.endsWith("@example.com")
message.trim().length > 0
// Note: message.trim() returns a call node, .length is member access on that
```

### Array Methods

```typescript
// Check membership
users.someEqual("role", "admin")
// AST: {
//   type: "call",
//   callee: {
//     type: "memberAccess",
//     object: { type: "identifier", name: "users" },
//     property: "someEqual"
//   },
//   arguments: [
//     { type: "literal", valueType: "string", value: "role" },
//     { type: "literal", valueType: "string", value: "admin" }
//   ],
//   methodKind: "arrayMethod"
// }

messages.countWhere("status", "read") > 5

// Aggregation
orders.sum("amount") > 1000
scores.avg("value") >= 60

// Transformation
items.map("name")
categories.distinct("type")
```

### Function Calls

```typescript
formatDate(user.createdAt, "YYYY-MM-DD")
// AST: {
//   type: "call",
//   callee: { type: "identifier", name: "formatDate" },
//   arguments: [
//     { type: "memberAccess", object: {...}, property: "createdAt" },
//     { type: "literal", valueType: "string", value: "YYYY-MM-DD" }
//   ]
// }

calculateTotal(order.items, order.discount)
```

### Ternary

```typescript
age >= 18 ? "adult" : "minor"
// AST: {
//   type: "ternary",
//   condition: { type: "binary", operator: ">=", ... },
//   consequent: { type: "literal", valueType: "string", value: "adult" },
//   alternate: { type: "literal", valueType: "string", value: "minor" }
// }

score > 60 ? "pass" : "fail"
```

### Complex Expressions

```typescript
// Mixed: array method with comparison
messages.countWhere("role", "user") > 5 && messages.someEqual("priority", "high")
// AST: {
//   type: "binary",
//   operator: "&&",
//   left: { type: "binary", operator: ">", ... },
//   right: { type: "binary", operator: "&&", ... }
// }

// Nested member access
user.address.city == "Beijing"
// AST: {
//   type: "binary",
//   operator: "==",
//   left: {
//     type: "memberAccess",
//     object: {
//       type: "memberAccess",
//       object: {
//         type: "memberAccess",
//         object: { type: "identifier", name: "user" },
//         property: "address"
//       },
//       property: "city"
//     },
//     property: "city"
//   },
//   right: { type: "literal", valueType: "string", value: "Beijing" }
// }

order.items.map("price").sum() > 1000
// Chained method calls: map returns array, sum operates on it

// Function call in comparison
calculateScore(user) > 60 || user.isAdmin == true
```

### Array Literals

```typescript
x in [1, 2, 3]
// AST: {
//   type: "binary",
//   operator: "in",
//   left: { type: "identifier", name: "x" },
//   right: {
//     type: "arrayLiteral",
//     elements: [
//       { type: "literal", valueType: "number", value: 1 },
//       { type: "literal", valueType: "number", value: 2 },
//       { type: "literal", valueType: "number", value: 3 }
//     ]
//   }
// }

[1, 2, 3].length > 0
// Empty array: []
```

## Migration: Regex Parser vs Chevrotain Grammar

| Aspect                | Current (Regex Parser)         | Target (Chevrotain Grammar)   |
|-----------------------|--------------------------------|-------------------------------|
| Parser Type           | Manual recursive descent       | Generated from rule definitions |
| Grammar Specification | Implicit in code               | Explicit RULE/SUBRULE structure |
| Operator Precedence   | Hard-coded ordering            | Grammar rule nesting          |
| Error Messages        | Limited                        | Position-aware, descriptive   |
| Extensibility         | Modify parser code             | Add grammar rules             |
| Performance           | ~50K ops/sec                   | Comparable or better          |
| Source Locations      | Manual tracking                | Automatic via token positions |
| Left-Associative Ops  | Manual folding                 | Built-in MANY() combinator    |
| AST Structure         | Fragmented (many node types)   | Unified (binary, call)        |
| Type Safety           | Runtime checks                 | TypeScript type enforcement   |
