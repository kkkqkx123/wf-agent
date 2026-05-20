# Condition Expression DSL Architecture Design (Chevrotain Implementation)

This document defines the architecture for the condition expression module using Chevrotain. The design prioritizes maintainability, extensibility, and clear separation of concerns with a unified AST structure.

## Architectural Vision

```
┌─────────────────────────────────────────────────────────────────────┐
│                      Condition Expression DSL                       │
│                                                                     │
│  ┌──────────┐    ┌──────────┐    ┌─────────┐    ┌──────────────┐  │
│  │  Input   │───▶│  Lexer   │───▶│ Parser  │───▶│  CST         │  │
│  │ (String) │    │ (Tokens) │    │ (Rules) │    │ (Syntax)     │  │
│  └──────────┘    └──────────┘    └─────────┘    └──────┬───────┘  │
│                                                         │          │
│                                                         ▼          │
│                                              ┌──────────────────┐  │
│                                              │   Visitor        │  │
│                                              │   (CST→AST)      │  │
│                                              └──────────────────┘  │
│                                                                    │
│                                                                    ▼
│                                              ┌──────────────────┐  │
│                                              │   AST            │  │
│                                              │   (Semantic)     │  │
│                                              └──────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
```

## Module Structure

The DSL module within `common-utils` follows this package layout:

```
src/evalutor/
├── index.ts                    # Public API exports
├── ast-types.ts                # AST type definitions (UPDATED)
├── ast-metadata.ts             # Metadata utilities (existing)
│
├── dsl/                        # NEW: DSL parser module (Chevrotain)
│   ├── index.ts                # DSL public API
│   ├── condition-lexer.ts      # Chevrotain lexer definition
│   ├── condition-parser.ts     # Chevrotain parser definition
│   ├── condition-cst-to-ast.ts # CST → AST visitor/converter
│   ├── tokens.ts               # Token type definitions
│   └── types.ts                # DSL-internal types
│
├── expression-compiler.ts      # Expression compiler (existing)
├── expression-evaluator.ts     # AST evaluator (UPDATED for new AST)
├── condition-evaluator.ts      # Condition evaluator (existing)
├── debug-tools.ts              # Debugging tools (existing)
├── dependency-tracker.ts       # Dependency tracking (existing)
├── path-resolver.ts            # Path resolution (existing)
├── security-validator.ts       # Security validation (existing)
└── type-validator.ts           # Type validation (existing)
```

## Processing Pipeline

### Stage 1: Lexer (Tokenization)

The lexer converts the raw expression string into a stream of tokens. Each token carries position information.

**Token Priority Order** (defined in `tokens.ts`):

```typescript
// Keywords first (higher priority than Identifier)
True, False, Null, Contains, In

// Predefined method names (avoid confusion with identifiers)
ArrayMethod, StringMethod

// Operators (by precedence group)
ComparisonOp, LogicalOr, LogicalAnd, Not
Plus, Minus, Multiply, Divide, Modulo
Ternary, Colon, Dot, LParen, RParen, LBracket, RBracket, Comma

// Literals
StringLiteral, NumberLiteral

// Identifiers (lowest priority - matches everything else)
Identifier

// Skipped
WhiteSpace, LineComment, BlockComment
```

**Example Tokenization:**

```
Input:  user.age >= 18 && name.contains("admin")
        │  │     │   │   │     │                  │
        ▼  ▼     ▼   ▼   ▼     ▼                  ▼
Tokens: [Identifier, Dot, Identifier, Gte, Number, And, Identifier, Dot, ArrayMethod, LParen, StringLiteral, RParen]
```

### Stage 2: Parser (CST Generation)

The parser consumes tokens and produces a Concrete Syntax Tree (CST) using Chevrotain's parsing rules. The CST preserves all syntactic detail including whitespace position and comments.

**Parser Rule Hierarchy (Chevrotain Style):**

```
expression
  └── ternary (right-associative)
        ├── logicalOr "?" expression ":" expression
        └── logicalAnd "||" ...
             ├── logicalAnd "&&" ...
             └── notExpr
                   ├── "!" notExpr
                   └── comparison
                         ├── addition comparisonOp addition
                         └── addition
                               ├── multiplication "+" addition
                               ├── multiplication "-" addition
                               └── multiplication
                                     ├── unary "*" ...
                                     ├── unary "/" ...
                                     ├── unary "%" ...
                                     ├── unary "-" unary (unary minus)
                                     └── primary
                                           ├── functionCall
                                           ├── arrayLiteral
                                           ├── literal (string, number, boolean, null)
                                           ├── memberAccess (with method calls)
                                           └── "(" expression ")"
```

**Key Grammar Features:**

- **Left-associative operators** (`+`, `-`, `*`, `/`, `&&`, `||`, comparisons): Implemented using `MANY()` in Chevrotain, producing flat arrays in CST that are folded into left-leaning trees during visitor conversion.
- **Right-associative operators** (`? :`, `!`, unary `-`): Implemented using recursive `SUBRULE()` calls.
- **Operator precedence**: Achieved through nested rule definitions (lower precedence rules call higher precedence rules as subrules).

### Stage 3: CST-to-AST Conversion

The CST is transformed into a semantic AST using Chevrotain's visitor pattern. This is where syntactic ambiguity is resolved and semantic meaning is assigned.

**Visitor Pattern Implementation:**

```typescript
class ConditionCstToAstVisitor extends CstVisitor {
  constructor() {
    super();
    this.validateVisitor();
  }

  // Entry point
  public visitExpression(ctx: CstNode): Expression {
    return this.visit(ctx.ternary);
  }

  // Ternary operator (right-associative)
  private visitTernary(ctx: any): Expression {
    if (ctx.logicalOr && !ctx.Ternary) {
      return this.visit(ctx.logicalOr[0]);
    }
    return {
      type: "ternary",
      condition: this.visit(ctx.logicalOr[0]),
      consequent: this.visit(ctx.ternary_consequent),
      alternate: this.visit(ctx.ternary_alternate),
      metadata: this.extractMetadata(ctx)
    };
  }

  // Left-associative OR (fold flat CST array into tree)
  private visitLogicalOr(ctx: any): Expression {
    let left = this.visit(ctx.logicalAnd[0]);
    for (let i = 1; i < ctx.logicalAnd.length; i++) {
      left = {
        type: "binary",
        operator: "||",
        left: left,
        right: this.visit(ctx.logicalAnd[i]),
        metadata: this.extractMetadata(ctx)
      };
    }
    return left;
  }

  // Similar patterns for logicalAnd, addition, multiplication, etc.
}
```

**Key Mappings:**

| CST Pattern | AST Node Produced |
|-------------|-------------------|
| `memberAccess` alone | `identifier` or `memberAccess` |
| `memberAccess.methodCall` | `call` with `callee.memberAccess` |
| `comparison` with binary operator | `binary` node |
| `identifier(args)` | `call` with `callee.identifier` |
| `primary.methodCall(args)` | `call` with `callee.memberAccess` |
| `"-" unary` | `unaryMinus` node |
| `"!" notExpr` | `not` node |

### Stage 4: Evaluation

The AST is evaluated by the updated `ExpressionEvaluator`. The evaluator walks the tree recursively, resolving variables from the `EvaluationContext`. Key changes:

- **Unified binary operations**: All comparisons, logic, and arithmetic use the same `binary` node type.
- **Unified calls**: Function calls, array methods, and string methods all use the `call` node type.
- **Explicit unary minus**: Separate `unaryMinus` node instead of `0 - x` hack.

## Unified AST Design (Updated)

### Core Types

```typescript
export interface SourceLocation { 
  start: number; 
  end: number; 
  line?: number; 
  column?: number; 
}

export interface NodeMetadata { 
  location?: SourceLocation; 
  comments?: string[]; 
  custom?: any; 
}

export type Expression =
  | LiteralExpr
  | IdentifierExpr
  | MemberAccessExpr
  | UnaryMinusExpr
  | BinaryExpr        // Unified: logic, arithmetic, comparison
  | NotExpr
  | TernaryExpr
  | CallExpr          // Unified: functions, array methods, string methods
  | ArrayLiteralExpr;
```

### Node Definitions

#### Literal Expressions

```typescript
export interface LiteralExpr {
  type: "literal";
  valueType: "boolean" | "number" | "string" | "null";
  value: any;
  metadata?: NodeMetadata;
}
```

#### Identifier Expressions

```typescript
export interface IdentifierExpr {
  type: "identifier";
  name: string;
  metadata?: NodeMetadata;
}
```

#### Member Access Expressions

```typescript
export interface MemberAccessExpr {
  type: "memberAccess";
  object: Expression;   // Can be IdentifierExpr or another MemberAccessExpr
  property: string;
  metadata?: NodeMetadata;
}
```

#### Unary Minus (NEW - replaces 0-x hack)

```typescript
export interface UnaryMinusExpr {
  type: "unaryMinus";
  operand: Expression;
  metadata?: NodeMetadata;
}
```

#### Binary Expressions (UNIFIED)

```typescript
export interface BinaryExpr {
  type: "binary";
  operator: BinaryOperator;
  left: Expression;
  right: Expression;
  metadata?: NodeMetadata;
}

export type BinaryOperator =
  // Comparison
  | "==" | "!=" | ">" | "<" | ">=" | "<=" | "contains" | "in"
  // Logical
  | "&&" | "||"
  // Arithmetic
  | "+" | "-" | "*" | "/" | "%";
```

#### NOT Expression

```typescript
export interface NotExpr {
  type: "not";
  operand: Expression;
  metadata?: NodeMetadata;
}
```

#### Ternary Expression

```typescript
export interface TernaryExpr {
  type: "ternary";
  condition: Expression;
  consequent: Expression;
  alternate: Expression;
  metadata?: NodeMetadata;
}
```

#### Call Expressions (UNIFIED)

```typescript
export interface CallExpr {
  type: "call";
  // Caller can be identifier (function), memberAccess (method), or any expression
  callee: Expression; 
  arguments: Expression[];
  metadata?: NodeMetadata;
  // Optional hint for type checking (doesn't affect structure)
  methodKind?: "arrayMethod" | "stringMethod" | "function";
}
```

**Examples:**

```typescript
// Function call: formatDate(user.createdAt)
{
  type: "call",
  callee: { type: "identifier", name: "formatDate" },
  arguments: [
    { type: "memberAccess", object: {...}, property: "createdAt" }
  ]
}

// Array method: users.someEqual("role", "admin")
{
  type: "call",
  callee: {
    type: "memberAccess",
    object: { type: "identifier", name: "users" },
    property: "someEqual"
  },
  arguments: [
    { type: "literal", valueType: "string", value: "role" },
    { type: "literal", valueType: "string", value: "admin" }
  ],
  methodKind: "arrayMethod"
}

// String method: name.startsWith("J")
{
  type: "call",
  callee: {
    type: "memberAccess",
    object: { type: "identifier", name: "name" },
    property: "startsWith"
  },
  arguments: [
    { type: "literal", valueType: "string", value: "J" }
  ],
  methodKind: "stringMethod"
}
```

#### Array Literal Expressions

```typescript
export interface ArrayLiteralExpr {
  type: "arrayLiteral";
  elements: Expression[];
  metadata?: NodeMetadata;
}
```

## Chevrotain-Specific Implementation Details

### Lexer Configuration

```typescript
import { createToken, Lexer, ITokenConfig } from "chevrotain";

// Keywords (must be defined before Identifier for priority)
const True = createToken({ 
  name: "True", 
  pattern: /true/, 
  longer_alt: Identifier 
});
const False = createToken({ 
  name: "False", 
  pattern: /false/, 
  longer_alt: Identifier 
});
const Null = createToken({ 
  name: "Null", 
  pattern: /null/, 
  longer_alt: Identifier 
});
const Contains = createToken({ 
  name: "Contains", 
  pattern: /contains/, 
  longer_alt: Identifier 
});
const In = createToken({ 
  name: "In", 
  pattern: /in/, 
  longer_alt: Identifier 
});

// Array method names (prevent confusion with regular identifiers)
const ArrayMethod = createToken({
  name: "ArrayMethod",
  pattern: /someEqual|someContains|everyEqual|everyHas|countWhere|countWhereContains|findEqual|findContains|has|hasContains|sum|avg|min|max|someGreaterThan|someLessThan|everyGreaterThan|everyLessThan|map|distinct|first|last/,
  longer_alt: Identifier
});

// String method names
const StringMethod = createToken({
  name: "StringMethod",
  pattern: /startsWith|endsWith|length|toLowerCase|toUpperCase|trim/,
  longer_alt: Identifier
});

// Operators
const ComparisonOp = createToken({ name: "ComparisonOp", pattern: /==|!=|>=|<=|>|</ });
const LogicalOr = createToken({ name: "LogicalOr", pattern: /\|\|/ });
const LogicalAnd = createToken({ name: "LogicalAnd", pattern: /&&/ });
const Not = createToken({ name: "Not", pattern: /!/ });
const Plus = createToken({ name: "Plus", pattern: /\+/ });
const Minus = createToken({ name: "Minus", pattern: /-/ });
const Multiply = createToken({ name: "Multiply", pattern: /\*/ });
const Divide = createToken({ name: "Divide", pattern: /\// });
const Modulo = createToken({ name: "Modulo", pattern: /%/ });
const Ternary = createToken({ name: "Ternary", pattern: /\?/ });
const Colon = createToken({ name: "Colon", pattern: /:/ });
const Dot = createToken({ name: "Dot", pattern: /\./ });
const LParen = createToken({ name: "LParen", pattern: /\(/ });
const RParen = createToken({ name: "RParen", pattern: /\)/ });
const LBracket = createToken({ name: "LBracket", pattern: /\[/ });
const RBracket = createToken({ name: "RBracket", pattern: /\]/ });
const Comma = createToken({ name: "Comma", pattern: /,/ });

// Literals
const StringLiteral = createToken({ name: "StringLiteral", pattern: /'([^'\\]|\\.)*'|"([^"]|\\.)*"/ });
const NumberLiteral = createToken({ name: "NumberLiteral", pattern: /\d+(\.\d+)?/ });

// Identifier (last - lowest priority)
const Identifier = createToken({ name: "Identifier", pattern: /[a-zA-Z_]\w*/ });

// Whitespace and comments (skipped)
const WhiteSpace = createToken({ name: "WhiteSpace", pattern: /\s+/, group: Lexer.SKIPPED });
const LineComment = createToken({ name: "LineComment", pattern: /\/\/[^\n\r]*/, group: Lexer.SKIPPED });
const BlockComment = createToken({ name: "BlockComment", pattern: /\/\*[\s\S]*?\*\//, group: Lexer.SKIPPED });

export const allTokens = [
  WhiteSpace, LineComment, BlockComment,
  True, False, Null, Contains, In,
  ArrayMethod, StringMethod,
  ComparisonOp, LogicalOr, LogicalAnd, Not,
  Plus, Minus, Multiply, Divide, Modulo,
  Ternary, Colon, Dot, LParen, RParen, LBracket, RBracket, Comma,
  StringLiteral, NumberLiteral,
  Identifier
];

export const conditionLexer = new Lexer(allTokens, {
  ensureOptimizations: true,
});
```

### Parser Configuration

```typescript
import { CstParser } from "chevrotain";

class ConditionParser extends CstParser {
  constructor() {
    super(allTokens);
    this.performSelfAnalysis();
  }

  // Top-level rule
  public expression = this.RULE("expression", () => {
    this.SUBRULE(this.ternary);
  });

  // Ternary (right-associative)
  private ternary = this.RULE("ternary", () => {
    this.OR([
      { ALT: () => {
          this.SUBRULE(this.logicalOr);
          this.CONSUME(Ternary);
          this.SUBRULE(this.ternary, { LABEL: "consequent" });
          this.CONSUME(Colon);
          this.SUBRULE(this.ternary, { LABEL: "alternate" });
        }
      },
      { ALT: () => this.SUBRULE(this.logicalOr) }
    ]);
  });

  // Logical OR (left-associative via MANY)
  private logicalOr = this.RULE("logicalOr", () => {
    this.SUBRULE(this.logicalAnd);
    this.MANY(() => {
      this.CONSUME(LogicalOr);
      this.SUBRULE2(this.logicalAnd);
    });
  });

  // Logical AND (left-associative)
  private logicalAnd = this.RULE("logicalAnd", () => {
    this.SUBRULE(this.notExpr);
    this.MANY(() => {
      this.CONSUME(LogicalAnd);
      this.SUBRULE2(this.notExpr);
    });
  });

  // NOT operator (right-associative)
  private notExpr = this.RULE("notExpr", () => {
    this.OR([
      { ALT: () => { this.CONSUME(Not); this.SUBRULE(this.notExpr); } },
      { ALT: () => this.SUBRULE(this.comparison) }
    ]);
  });

  // Comparison operators
  private comparison = this.RULE("comparison", () => {
    this.SUBRULE(this.addition);
    this.OPTION(() => {
      this.CONSUME(ComparisonOp);
      this.SUBRULE2(this.addition);
    });
  });

  // Addition/Subtraction (left-associative)
  private addition = this.RULE("addition", () => {
    this.SUBRULE(this.multiplication);
    this.MANY(() => {
      this.OR([
        { ALT: () => this.CONSUME(Plus) },
        { ALT: () => this.CONSUME(Minus) }
      ]);
      this.SUBRULE2(this.multiplication);
    });
  });

  // Multiplication/Division/Modulo (left-associative)
  private multiplication = this.RULE("multiplication", () => {
    this.SUBRULE(this.unary);
    this.MANY(() => {
      this.OR([
        { ALT: () => this.CONSUME(Multiply) },
        { ALT: () => this.CONSUME(Divide) },
        { ALT: () => this.CONSUME(Modulo) }
      ]);
      this.SUBRULE2(this.unary);
    });
  });

  // Unary minus (right-associative)
  private unary = this.RULE("unary", () => {
    this.OR([
      { ALT: () => { this.CONSUME(Minus); this.SUBRULE(this.unary); } },
      { ALT: () => this.SUBRULE(this.primary) }
    ]);
  });

  // Primary expressions
  private primary = this.RULE("primary", () => {
    this.OR([
      { ALT: () => this.SUBRULE(this.functionCall) },
      { ALT: () => this.SUBRULE(this.arrayLiteral) },
      { ALT: () => this.CONSUME(StringLiteral) },
      { ALT: () => this.CONSUME(NumberLiteral) },
      { ALT: () => this.CONSUME(True) },
      { ALT: () => this.CONSUME(False) },
      { ALT: () => this.CONSUME(Null) },
      { ALT: () => this.SUBRULE(this.memberAccess) },
      { ALT: () => { this.CONSUME(LParen); this.SUBRULE(this.expression); this.CONSUME(RParen); } }
    ]);
  });

  // Member access chain (obj.prop1.prop2.method())
  private memberAccess = this.RULE("memberAccess", () => {
    this.SUBRULE(this.primary);
    this.MANY(() => {
      this.CONSUME(Dot);
      this.OR([
        { ALT: () => this.SUBRULE(this.methodCall) },
        { ALT: () => this.CONSUME(Identifier, { LABEL: "property" }) }
      ]);
    });
  });

  // Method call (array/string methods)
  private methodCall = this.RULE("methodCall", () => {
    this.OR([
      { ALT: () => this.CONSUME(ArrayMethod) },
      { ALT: () => this.CONSUME(StringMethod) }
    ]);
    this.CONSUME(LParen);
    this.SUBRULE(this.argumentList);
    this.CONSUME(RParen);
  });

  // Function call
  private functionCall = this.RULE("functionCall", () => {
    this.CONSUME(Identifier);
    this.CONSUME(LParen);
    this.SUBRULE(this.argumentList);
    this.CONSUME(RParen);
  });

  // Array literal
  private arrayLiteral = this.RULE("arrayLiteral", () => {
    this.CONSUME(LBracket);
    this.OPTION(() => this.SUBRULE(this.valueList));
    this.CONSUME(RBracket);
  });

  // Value list (for arrays)
  private valueList = this.RULE("valueList", () => {
    this.SUBRULE(this.literal);
    this.MANY(() => {
      this.CONSUME(Comma);
      this.SUBRULE2(this.literal);
    });
  });

  // Argument list (for calls)
  private argumentList = this.RULE("argumentList", () => {
    this.OPTION(() => {
      this.SUBRULE(this.expression);
      this.MANY(() => {
        this.CONSUME(Comma);
        this.SUBRULE2(this.expression);
      });
    });
  });

  // Literals
  private literal = this.RULE("literal", () => {
    this.OR([
      { ALT: () => this.CONSUME(StringLiteral) },
      { ALT: () => this.CONSUME(NumberLiteral) },
      { ALT: () => this.CONSUME(True) },
      { ALT: () => this.CONSUME(False) },
      { ALT: () => this.CONSUME(Null) }
    ]);
  });
}
```

## Error Handling Strategy

### Error Types

| Error Type         | Source      | Description                        | Example                    |
|--------------------|-------------|------------------------------------|----------------------------|
| `LexError`         | Lexer       | Invalid token sequence             | Unclosed string literal    |
| `ParseError`       | Parser      | Unexpected token                   | Missing operand            |
| `SemanticError`    | CST→AST     | Semantic constraint violation      | Unknown array method name  |
| `EvaluationError`  | Evaluator   | Runtime evaluation failure         | Type mismatch, div by zero |

### Recovery Mechanism

Chevrotain provides built-in error recovery:

```typescript
// Enable error recovery in parser
const parser = new ConditionParser({
  recoveryEnabled: true,
  maxLookahead: 1,
});

// Get detailed error information
const result = parser.parse(input);
if (parser.errors.length > 0) {
  const errors = parser.errors.map(err => ({
    message: err.message,
    offset: err.offset,
    line: err.line,
    column: err.column,
    severity: "error"
  }));
}
```

### Error Message Format

```typescript
interface DslError {
  message: string;
  location: {
    startOffset: number;
    endOffset: number;
    line: number;
    column: number;
  };
  suggestions?: string[];
  severity: "error" | "warning";
}
```

## Performance Considerations

| Aspect               | Current (Regex) | Target (Chevrotain) |
|----------------------|-----------------|---------------------|
| Parsing time (avg)   | ~0.02ms         | ~0.015ms (expected) |
| Tokenization         | Implicit        | Explicit, fast      |
| Error recovery       | None            | Token insertion/delete|
| Grammar validation   | Runtime         | Self-analysis at init|
| Memory (per parse)   | AST only        | CST + AST (larger)  |

**Optimization strategies:**
- CST-to-AST conversion can be skipped in evaluation-only mode
- Pre-compilation of frequently used expressions
- Result caching for pure expressions
- Reuse parser instances (thread-safe after initialization)

## API Design

### Public API (unchanged from current)

```typescript
// Parse expression to AST
function parseAST(expression: string): ASTNode;

// Evaluate expression
class ExpressionEvaluator {
  evaluate(expression: string, context: EvaluationContext): unknown;
  registerFunction(name: string, fn: Function): void;
}

// Evaluate condition
class ConditionEvaluator {
  evaluate(condition: Condition, context: EvaluationContext): boolean;
}
```

### Internal DSL API

```typescript
// === DSL Module Internal API (src/evalutor/dsl/) ===

// Parse expression to CST
function parseToCst(expression: string): CstNode;

// Convert CST to AST
function cstToAst(cst: CstNode): ASTNode;

// Shorthand: parse string to AST (full pipeline)
function dslParse(expression: string): ASTNode;

// Get detailed error with position info
function dslParseWithErrors(expression: string): {
  ast: ASTNode | null;
  errors: DslError[];
};

// Validate expression without full evaluation
function dslValidate(expression: string): {
  valid: boolean;
  errors: DslError[];
};
```

## Migration Compatibility

The DSL parser must maintain backward compatibility with all existing expression syntax:

| Expression                           | Current Support | DSL Support | Phase     |
|--------------------------------------|-----------------|-------------|-----------|
| `x == 5`                             | ✅              | ✅           | Phase 2   |
| `x > 5 && y < 10`                    | ✅              | ✅           | Phase 2   |
| `!isDeleted`                         | ✅              | ✅           | Phase 2   |
| `name contains "admin"`              | ✅              | ✅           | Phase 2   |
| `x in [1, 2, 3]`                     | ✅              | ✅           | Phase 2   |
| `user.age + 1`                       | ✅              | ✅           | Phase 2   |
| `name.startsWith("J")`        | ✅              | ✅           | Phase 2   |
| `age >= 18 ? "adult" : "minor"`     | ✅              | ✅           | Phase 2   |
| `items.someEqual("role", "admin")`   | ✅              | ✅           | Phase 2   |
| `items.countWhere("x", 1) > 5`      | ✅              | ✅           | Phase 2   |
| `formatDate(user.createdAt)`         | ✅              | ✅           | Phase 2   |
| `user.address.city`                  | ✅              | ✅           | Phase 2   |
| Lambda expressions                   | ❌              | ✅ (Phase 3) | Phase 3   |
| Object literals                      | ❌              | ✅ (Phase 3) | Phase 3   |
| Try-catch expressions                | ❌              | ✅ (Phase 3) | Phase 3   |

## Testing Strategy

### Unit Tests

```typescript
// Test lexer
describe("ConditionLexer", () => {
  it("should tokenize comparison expression", () => {
    const result = conditionLexer.tokenize("user.age >= 18");
    expect(result.tokens).toHaveLength(5);
  });

  it("should report error on invalid input", () => {
    const result = conditionLexer.tokenize("@invalid");
    expect(result.errors).toHaveLength(1);
  });
});

// Test parser
describe("ConditionParser", () => {
  it("should parse logical expression", () => {
    const cst = parseToCst("a > 5 && b < 10");
    expect(cst).toMatchSnapshot();
  });
});

// Test CST-to-AST
describe("ConditionCstToAstVisitor", () => {
  it("should produce correct AST for binary expression", () => {
    const ast = dslParse("age >= 18");
    expect(ast).toEqual({
      type: "binary",
      operator: ">=",
      left: { type: "identifier", name: "age" },
      right: { type: "literal", valueType: "number", value: 18 }
    });
  });

  it("should handle left-associative operators correctly", () => {
    const ast = dslParse("a + b + c");
    // Should be ((a + b) + c), not (a + (b + c))
    expect(ast.type).toBe("binary");
    expect(ast.operator).toBe("+");
    expect(ast.left.type).toBe("binary");
    expect(ast.left.operator).toBe("+");
  });
});

// Round-trip test: compare with existing parser
describe("DSL Compatibility", () => {
  it.each(EXISTING_EXPRESSIONS)("should parse '%s' identically", (expr) => {
    const oldAst = parseAST(expr);
    const newAst = dslParse(expr);
    expect(newAst).toDeepEqual(oldAst);
  });
});
```

### Fuzz Testing

```typescript
// Verify robustness against malformed input
describe("DSL Error Recovery", () => {
  it("should handle missing closing paren", () => {
    const result = dslParseWithErrors("(a > 5");
    expect(result.errors).toHaveLength(1);
    expect(result.errors[0].location).toBeDefined();
  });

  it("should handle unclosed string literal", () => {
    const result = dslParseWithErrors('"unclosed');
    expect(result.errors).toHaveLength(1);
  });

  it("should handle invalid operator", () => {
    const result = dslParseWithErrors("a @@ b");
    expect(result.errors).toHaveLength(1);
  });
});
```
