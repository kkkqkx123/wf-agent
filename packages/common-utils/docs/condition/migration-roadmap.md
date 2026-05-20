# Condition DSL Migration Roadmap (Chevrotain Implementation)

## Executive Summary

Migrate the condition expression module from a manual regex-based parser to a formal Chevrotain-based DSL parser with a unified AST structure. Total timeline: 12-18 weeks.

**Key Improvements:**
- Unified AST design (removes fragmented node types)
- Better error recovery and position-aware error messages
- Left-associative operator support via Chevrotain's `MANY()` combinator
- Explicit unary minus node (removes `0 - x` hack)
- Cleaner separation of concerns between lexer, parser, and evaluator
- Extensibility for future features (lambda expressions, object literals, etc.)

## Phase Overview

### Phase 0: Environment Setup & Proof of Concept (Weeks 1-2)

**Goal**: Set up Chevrotain development environment and validate technical feasibility with PoCs.

**Tasks:**
```bash
# Install dependencies
pnpm add chevrotain
pnpm add --save-dev @types/chevrotain

# Create initial project structure
mkdir -p src/dsl
touch src/dsl/index.ts src/dsl/tokens.ts src/dsl/condition-parser.ts
```

**Deliverables:**
- ✅ Chevrotain integration complete, compilation passes
- ✅ Two PoCs:
  1. Core expression parsing (basic operators, precedence)
  2. CST-to-AST conversion with visitor pattern
- ✅ Performance baseline report (compare with existing regex parser)
- ✅ Final technology selection decision (Chevrotain confirmed)

**Success Criteria:**
- Basic expressions parse correctly: `a + b * c`, `x > 5 && y < 10`
- Error messages include line/column information
- Parsing performance within 20% of current implementation

---

### Phase 1: Core DSL Framework (Weeks 3-6)

**Goal**: Implement lexer, parser, and CST→AST conversion framework covering all existing syntax.

#### Week 3: Lexer Definition + All Token Types

**Tasks:**
- Define all token types in `tokens.ts` with correct priority order
- Implement keyword handling (`true`, `false`, `null`, `contains`, `in`)
- Implement array method tokens (`someEqual`, `sum`, `avg`, etc.)
- Implement string method tokens (`startsWith`, `endsWith`, etc.)
- Configure lexer with whitespace/comment skipping
- Write unit tests for tokenization

**Validation Standard:**
- All existing expressions can be correctly tokenized
- No ambiguous token conflicts
- Position information preserved on all tokens

**Key Files:**
- `src/dsl/tokens.ts` - Complete token definitions
- `__tests__/dsl/tokens.test.ts` - Tokenization tests

#### Week 4: Parser Core Rules (Comparison/Logic)

**Tasks:**
- Implement parser class with self-analysis
- Define top-level `expression` rule
- Implement ternary operator (right-associative)
- Implement logical OR/AND (left-associative via `MANY()`)
- Implement NOT operator (right-associative)
- Implement comparison operators
- Write integration tests comparing output with existing parser

**Validation Standard:**
- Basic expressions parse with same AST structure as old parser
- Operator precedence is correct
- Left-associative operators fold correctly

**Key Files:**
- `src/dsl/condition-parser.ts` - Parser rules
- `__tests__/dsl/parser.test.ts` - Parser tests

#### Week 5: Complete Parser Rules + CST→AST Conversion

**Tasks:**
- Implement arithmetic operators (+, -, *, /, %)
- Implement unary minus (new `unaryMinus` node type)
- Implement primary expressions (literals, identifiers, member access)
- Implement function calls, array methods, string methods
- Implement array literals
- Build CST→AST visitor with proper folding logic
- Handle left-associative operator tree construction

**Validation Standard:**
- All 9 AST node types generated correctly:
  - `literal`, `identifier`, `memberAccess`, `binary`, `not`, `unaryMinus`, `ternary`, `call`, `arrayLiteral`
- Round-trip test: new AST matches old AST for all existing expressions

**Key Files:**
- `src/dsl/condition-cst-to-ast.ts` - Visitor implementation
- `__tests__/dsl/cst-to-ast.test.ts` - Conversion tests

#### Week 6: Integration Testing + Round-Trip Validation

**Tasks:**
- Create compatibility adapter layer (`parseAST()` wrapper)
- Run full regression suite (36+ existing unit tests)
- Fix any discrepancies between old and new AST structures
- Document breaking changes (if any)
- Update documentation

**Validation Standard:**
- All existing 36+ unit tests pass
- Zero AST structural differences for supported expressions
- Performance within acceptable range

**Key Files:**
- `src/evaluator/expression-parser.ts` - Adapter layer
- `__tests__/dsl/compatibility.test.ts` - Round-trip tests

**Deliverables:**
- ✅ Complete `src/evalutor/dsl/` directory implementation
- ✅ Compatibility adapter layer (enables seamless `parseAST()` switch)
- ✅ Full test coverage for core functionality

---

### Phase 2: Quality Hardening (Weeks 7-10)

**Goal**: Improve error localization, add security validation, and comprehensive testing.

#### Week 7: Enhanced Error Messages

**Tasks:**
- Implement position-aware error reporting (line/column/offset)
- Add error suggestions for common mistakes
- Integrate with existing error handling system
- Create error message formatting utilities

**Deliverable:**
```typescript
interface DslError {
  message: string;
  location: {
    startOffset: number;
    endOffset: number;
    line: number;
    column: number;
  };
  suggestions?: string[];  // e.g., "Did you mean 'contains' instead of 'contain'?"
  severity: "error" | "warning";
}
```

#### Week 8: Boundary Case Testing

**Tasks:**
- Add edge case tests (empty arrays, nested parentheses, chained methods)
- Test maximum nesting depth limits
- Verify Unicode identifier support
- Test special characters in strings

**Deliverable:**
- Comprehensive test suite with 100+ test cases
- Snapshot tests for complex expressions

#### Week 9: Fuzz Testing & Security Audit

**Tasks:**
- Implement fuzz testing with malformed inputs
- Security audit for injection vulnerabilities
- Prototype pollution prevention checks
- Memory safety verification (no leaks in long-running processes)

**Deliverable:**
- Fuzz test suite with 10,000+ random inputs
- Security audit report
- Mitigation strategies documented

#### Week 10: Developer Documentation

**Tasks:**
- Write migration guide for developers
- Document new AST types and their semantics
- Create examples for common patterns
- Update API reference

**Deliverable:**
- Developer migration guide
- API documentation with examples
- Troubleshooting FAQ

**Success Criteria:**
- Test coverage > 95%
- Error message quality score: Excellent
- Security audit: No critical issues
- Developer migration guide published

---

### Phase 3: Advanced Features (Weeks 11-14)

**Goal**: Leverage the new DSL architecture to implement Phase 3 advanced features that were difficult or impossible with the old architecture.

#### Week 11: Lambda Expressions

**Implementation Path:**
- Add lambda syntax tokens (`λ`, `=>`, parameter list)
- Extend grammar to support lambda expressions
- Update visitor to generate `lambda` AST node
- Enhance evaluator to handle lambda execution

**Example:**
```typescript
// Syntax: λ(x) => x > 5
users.filter(λ(x) => x > 5)

// AST:
{
  type: "call",
  callee: { type: "memberAccess", object: {...}, property: "filter" },
  arguments: [
    {
      type: "lambda",
      parameters: [{ name: "x" }],
   body: { type: "binary", operator: ">", left: {...}, right: {...} }
    }
  ]
}
```

#### Week 12: Object Literals

**Implementation Path:**
- Add object literal tokens (`{`, `}`, `:`)
- Extend grammar to support key-value pairs
- Generate `objectLiteral` AST node
- Update evaluator for object access

**Example:**
```typescript
// Syntax: { name: "John", age: 30 }
const person = { name: "John", age: 30 }

// AST:
{
  type: "objectLiteral",
  properties: [
    { key: "name", value: { type: "literal", value: "John" } },
    { key: "age", value: { type: "literal", value: 30 } }
  ]
}
```

#### Week 13: Try-Catch Expressions

**Implementation Path:**
- Add try-catch keywords (`try`, `catch`, `throw`)
- Extend grammar for exception handling
- Generate `tryCatch` AST node
- Update evaluator with error handling logic

**Example:**
```typescript
// Syntax: try { riskyOp() } catch (e) fallbackValue
const result = try { parseJson(input) } catch (e) null
```

#### Week 14: Compilation Optimization

**Implementation Path:**
- Implement AST→JavaScript code generation
- Add caching mechanism for compiled expressions
- Benchmark compilation vs interpretation trade-offs
- Optimize hot paths

**Deliverables:**
- Lambda expression support
- Object literal support
- Try-catch expression support
- Expression compilation pipeline
- Performance benchmark report

---

### Phase 4: Legacy Parser Deprecation (Weeks 15-18)

**Goal**: After stable operation, remove legacy parser code and彻底 clean up technical debt.

#### Week 15: Gray Switch Strategy

**Tasks:**
- Enable new parser by default
- Keep old parser as fallback (feature flag)
- Monitor error rates and performance metrics
- Collect user feedback

**Configuration:**
```typescript
// Feature flag for gradual rollout
const useNewDsl = process.env.USE_NEW_DSL !== "false";

if (useNewDsl) {
  return dslParse(expression);
} else {
  return legacyParse(expression);
}
```

#### Week 16: Monitoring & Stability Verification

**Tasks:**
- Track error rates (should be < 0.1%)
- Monitor parsing performance (should match baseline)
- Verify no regressions in production workflows
- Address any reported issues

**Deliverable:**
- Stability report with metrics
- Issue tracking dashboard

#### Week 17: Legacy Code Removal

**Tasks:**
- Remove `expression-parser.ts` legacy implementation
- Clean up all `__MEMBER_ACCESS__:` hack markers
- Remove deprecated AST node types
- Update imports across codebase

**Deliverable:**
- Clean codebase without legacy parser
- Updated import statements

#### Week 18: Final Cleanup & Documentation

**Tasks:**
- Remove temporary compatibility layers
- Update version numbers
- Publish release notes
- Archive old documentation

**Deliverable:**
- Production-ready DSL parser
- Complete migration documentation
- Release notes v2.0.0

---

## Risk Assessment

| Risk                      | Probability | Impact | Mitigation Strategy                          |
|---------------------------|-------------|--------|----------------------------------------------|
| Grammar compatibility gap | Medium      | High   | Round-trip testing + gray rollout            |
| Performance regression    | Low         | High   | Continuous benchmarking, optimize CST→AST    |
| Team learning curve       | Medium      | Medium | PoC first + pair programming                 |
| Existing feature regression | Low       | Critical | Full regression suite +新旧对比验证        |
| Error recovery gaps       | Medium      | Medium | Fuzz testing + user feedback loop            |
| Security vulnerabilities  | Low         | High   | Security audit + input sanitization          |

---

## Decision Records

### Date: 2026-05-19

**Decision Point**: Parser Framework Selection

**Options:**
- **Chevrotain**: TypeScript-native, excellent error recovery, active maintenance
- **Peggy.js**: PEG-based, simpler grammar syntax, less TypeScript integration
- **Nearley**: Functional approach, smaller footprint, steeper learning curve

**Conclusion**: **Chevrotain** selected
- Best TypeScript support (native types, no transpilation needed)
- Superior error recovery mechanisms
- Active community and maintenance
- Proven track record in production systems

---

### Date: 2026-05-19

**Decision Point**: Migration Strategy

**Options:**
- **Progressive**: Gradual replacement with compatibility layer (recommended)
- **Hybrid**: Run both parsers in parallel, compare outputs
- **Status Quo**: Maintain existing parser indefinitely

**Conclusion**: **Progressive migration** selected
- Lower risk (can rollback at any phase)
- Allows incremental testing and validation
- Minimal disruption to existing workflows
- Clear milestones and deliverables

---

### Date: 2026-05-19

**Decision Point**: Architecture Layering

**Options:**
- **Independent DSL Submodule**: Separate `dsl/` directory alongside old implementation
- **Inline Refactoring**: Replace old parser in-place
- **Standalone Package**: Extract into separate npm package

**Conclusion**: **Independent DSL submodule** selected
- Maintains backward compatibility during transition
- Enables side-by-side comparison and testing
- Clear ownership and responsibility boundaries
- Easier to revert if issues arise

---

## Milestone Checklist

### Phase 0 (Weeks 1-2)
- [ ] Chevrotain installed and configured
- [ ] PoC 1: Basic expression parsing works
- [ ] PoC 2: CST→AST conversion works
- [ ] Performance baseline established
- [ ] Technology selection documented

### Phase 1 (Weeks 3-6)
- [ ] All token types defined and tested
- [ ] Parser rules implemented for all operators
- [ ] CST→AST visitor complete
- [ ] Round-trip compatibility verified
- [ ] All 36+ existing tests pass

### Phase 2 (Weeks 7-10)
- [ ] Error messages include position info
- [ ] Boundary case tests added
- [ ] Fuzz testing completed
- [ ] Security audit passed
- [ ] Developer documentation published
- [ ] Test coverage > 95%

### Phase 3 (Weeks 11-14)
- [ ] Lambda expressions implemented
- [ ] Object literals implemented
- [ ] Try-catch expressions implemented
- [ ] Compilation optimization working
- [ ] Performance benchmarks documented

### Phase 4 (Weeks 15-18)
- [ ] Gray switch enabled
- [ ] Stability metrics meet targets
- [ ] Legacy parser removed
- [ ] All hack markers cleaned up
- [ ] Release notes published
- [ ] Final documentation updated

---

## Success Metrics

### Technical Metrics
- **Parsing Performance**: ≤ 15ms for complex expressions (baseline: 20ms)
- **Test Coverage**: ≥ 95% (current: ~80%)
- **Error Recovery**: 100% of syntax errors produce meaningful messages
- **Memory Usage**: ≤ 2x baseline (CST overhead acceptable)

### Quality Metrics
- **Bug Rate**: ≤ 0.1% in production (post-migration)
- **User Satisfaction**: ≥ 4.5/5 on developer experience survey
- **Documentation Completeness**: 100% of features documented

### Business Metrics
- **Migration Timeline**: 18 weeks (±2 weeks)
- **Rollback Time**: ≤ 4 hours if critical issue arises
- **Feature Delivery**: Lambda/object literals delivered on schedule

---

## Resource Requirements

### Personnel
- **1 Senior Engineer**: Lead architect and implementation
- **1 Mid-Level Engineer**: Testing and documentation
- **1 QA Engineer**: Fuzz testing and validation

### Tools & Infrastructure
- Chevrotain library (open source)
- Vitest for testing (already in use)
- CI/CD pipeline for automated testing
- Performance monitoring dashboard

### Time Allocation
- **Phase 0**: 2 weeks (part-time, proof of concept)
- **Phase 1**: 4 weeks (full-time implementation)
- **Phase 2**: 4 weeks (testing and hardening)
- **Phase 3**: 4 weeks (advanced features)
- **Phase 4**: 4 weeks (deprecation and cleanup)

---

## Appendix A: AST Type Changes Summary

### Before (Fragmented AST)
```typescript
interface ComparisonNode {
  type: "comparison";
  variablePath: string | MemberAccessNode;  // Inconsistent!
  operator: string;
  value: Expression;
}

interface ArrayMethodNode {
  type: "arrayMethod";
  method: string;
  arrayPath: string;  // Hack: stored as string
  propertyName: string;
}

interface UnaryMinusHackNode {
  type: "arithmetic";
  operator: "-";
  left: { type: "number", value: 0 };  // Hack!
  right: Expression;
}
```

### After (Unified AST)
```typescript
// All binary operations unified
interface BinaryExpr {
  type: "binary";
  operator: BinaryOperator;  // Unified enum
  left: Expression;
  right: Expression;
}

// All calls unified
interface CallExpr {
  type: "call";
  callee: Expression;  // Can be identifier or memberAccess
  arguments: Expression[];
  methodKind?: "arrayMethod" | "stringMethod" | "function";
}

// Explicit unary minus
interface UnaryMinusExpr {
  type: "unaryMinus";
  operand: Expression;
}
```

**Benefits:**
- Simpler evaluator logic (fewer node types to handle)
- Consistent structure (all binaries have left/right)
- No hacks or workarounds
- Easier to extend (add new operators without new node types)

---

## Appendix B: Example Migration Path

### Old Parser Output (for `users.someEqual("role", "admin")`)
```json
{
  "type": "arrayMethodComparison",
  "methodNode": {
    "type": "arrayMethod",
    "method": "someEqual",
    "arrayPath": "...",
    "propertyName": "role",
    "value": "admin"
  },
  "operator": ">",
  "compareValue": {
    "type": "number",
    "value": 0
  }
}
```

### New Parser Output (Unified AST)
```json
{
  "type": "binary",
  "operator": ">",
  "left": {
    "type": "call",
    "callee": {
      "type": "memberAccess",
      "object": {
        "type": "identifier",
        "name": "users"
      },
      "property": "someEqual"
    },
    "arguments": [
      {
        "type": "literal",
        "valueType": "string",
        "value": "role"
      },
      {
        "type": "literal",
        "valueType": "string",
        "value": "admin"
      }
    ],
    "methodKind": "arrayMethod"
  },
  "right": {
    "type": "literal",
    "valueType": "number",
    "value": 0
  }
}
```

**Evaluator Adaptation:**
```typescript
// Old evaluator had special case for arrayMethodComparison
if (node.type === "arrayMethodComparison") {
  return this.evaluateArrayMethodComparison(node);
}

// New evaluator handles uniformly
if (node.type === "binary") {
  const left = this.evaluate(node.left, context);
  const right = this.evaluate(node.right, context);
  return this.applyBinaryOperator(node.operator, left, right);
}

// Call evaluation handles all method types
if (node.type === "call") {
  const callee = this.evaluate(node.callee, context);
  const args = node.arguments.map(arg => this.evaluate(arg, context));
  
  if (node.methodKind === "arrayMethod") {
    return this.executeArrayMethod(callee, args);
  } else if (node.methodKind === "stringMethod") {
    return this.executeStringMethod(callee, args);
  } else {
    return this.executeFunction(callee, args);
  }
}
```

This migration path demonstrates how the unified AST simplifies the evaluator while maintaining full functionality.
