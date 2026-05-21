import { CstParser, CstNode, Lexer } from "chevrotain";
import {
  allTokens,
  True,
  False,
  Null,
  Contains,
  In,
  ArrayMethod,
  StringMethod,
  ComparisonOp,
  LogicalOr,
  LogicalAnd,
  Not,
  Plus,
  Minus,
  Multiply,
  Divide,
  Modulo,
  Ternary,
  Colon,
  Dot,
  LParen,
  RParen,
  LBracket,
  RBracket,
  Comma,
  StringLiteral,
  NumberLiteral,
  Identifier,
} from "./tokens.js";

/**
 * Condition expression parser using Chevrotain
 *
 * Grammar precedence (lowest to highest):
 * 1. Ternary (?:) - right-associative
 * 2. Logical OR (||) - left-associative
 * 3. Logical AND (&&) - left-associative
 * 4. NOT (!) - right-associative
 * 5. Comparison (==, !=, >, <, >=, <=, contains, in) - left-associative
 * 6. Addition/Subtraction (+, -) - left-associative
 * 7. Multiplication/Division/Modulo (*, /, %) - left-associative
 * 8. Unary minus (-) - right-associative
 * 9. Member access (identifier.prop, method calls, subscript) - left-associative
 * 10. Primary (literals, functionCall, arrayLiteral, identifier, group)
 */
export class ConditionParser extends CstParser {
  public expression!: () => CstNode;
  private ternary!: () => CstNode;
  private logicalOr!: () => CstNode;
  private logicalAnd!: () => CstNode;
  private notExpr!: () => CstNode;
  private comparison!: () => CstNode;
  private addition!: () => CstNode;
  private multiplication!: () => CstNode;
  private unary!: () => CstNode;
  private memberAccess!: () => CstNode;
  private dotAccess!: () => CstNode;
  private primary!: () => CstNode;
  private methodCall!: () => CstNode;
  private subscript!: () => CstNode;
  private identifierExpr!: () => CstNode;
  private arrayLiteral!: () => CstNode;
  private valueList!: () => CstNode;
  private argumentList!: () => CstNode;
  private literal!: () => CstNode;

  constructor() {
    super(allTokens, {
      recoveryEnabled: true,
    });

    this.expression = this.RULE("expression", () => {
      this.SUBRULE(this.ternary);
    });

    this.ternary = this.RULE("ternary", () => {
      this.SUBRULE(this.logicalOr);
      this.OPTION(() => {
        this.CONSUME(Ternary);
        this.SUBRULE(this.ternary, { LABEL: "consequent" });
        this.CONSUME(Colon);
        this.SUBRULE2(this.ternary, { LABEL: "alternate" });
      });
    });

    this.logicalOr = this.RULE("logicalOr", () => {
      this.SUBRULE(this.logicalAnd);
      this.MANY(() => {
        this.CONSUME(LogicalOr);
        this.SUBRULE2(this.logicalAnd);
      });
    });

    this.logicalAnd = this.RULE("logicalAnd", () => {
      this.SUBRULE(this.notExpr);
      this.MANY(() => {
        this.CONSUME(LogicalAnd);
        this.SUBRULE2(this.notExpr);
      });
    });

    this.notExpr = this.RULE("notExpr", () => {
      this.OR([
        {
          ALT: () => {
            this.CONSUME(Not);
            this.SUBRULE(this.notExpr);
          },
        },
        { ALT: () => this.SUBRULE(this.comparison) },
      ]);
    });

    // Comparison operators (==, !=, >, <, >=, <=, contains, in)
    this.comparison = this.RULE("comparison", () => {
      this.SUBRULE(this.addition);
      this.OPTION(() => {
        this.OR([
          { ALT: () => this.CONSUME(ComparisonOp) },
          { ALT: () => this.CONSUME(Contains) },
          { ALT: () => this.CONSUME(In) },
        ]);
        this.SUBRULE2(this.addition);
      });
    });

    this.addition = this.RULE("addition", () => {
      this.SUBRULE(this.multiplication);
      this.MANY(() => {
        this.OR([{ ALT: () => this.CONSUME(Plus) }, { ALT: () => this.CONSUME(Minus) }]);
        this.SUBRULE2(this.multiplication);
      });
    });

    this.multiplication = this.RULE("multiplication", () => {
      this.SUBRULE(this.unary);
      this.MANY(() => {
        this.OR([
          { ALT: () => this.CONSUME(Multiply) },
          { ALT: () => this.CONSUME(Divide) },
          { ALT: () => this.CONSUME(Modulo) },
        ]);
        this.SUBRULE2(this.unary);
      });
    });

    // Unary minus (right-associative) - delegates to memberAccess
    this.unary = this.RULE("unary", () => {
      this.OR([
        {
          ALT: () => {
            this.CONSUME(Minus);
            this.SUBRULE(this.unary);
          },
        },
        { ALT: () => this.SUBRULE(this.memberAccess) },
      ]);
    });

    // Member access chain: identifier(.prop|.methodCall|[subscript])*
    this.memberAccess = this.RULE("memberAccess", () => {
      this.SUBRULE(this.primary);
      this.MANY(() => {
        this.OR([
          {
            ALT: () => {
              this.CONSUME(Dot);
              this.SUBRULE(this.dotAccess);
            },
          },
          {
            ALT: () => this.SUBRULE(this.subscript),
          },
        ]);
      });
    });

    this.dotAccess = this.RULE("dotAccess", () => {
      this.OR([
        { ALT: () => this.SUBRULE(this.methodCall) },
        { ALT: () => this.CONSUME(Identifier, { LABEL: "property" }) },
        { ALT: () => this.CONSUME(ArrayMethod, { LABEL: "property" }) },
        { ALT: () => this.CONSUME(StringMethod, { LABEL: "property" }) },
      ]);
    });

    this.subscript = this.RULE("subscript", () => {
      this.CONSUME(LBracket);
      this.OR([
        { ALT: () => this.CONSUME(NumberLiteral) },
        { ALT: () => this.CONSUME(StringLiteral) },
        { ALT: () => this.CONSUME(Identifier) },
      ]);
      this.CONSUME(RBracket);
    });

    // Primary expressions (no recursion back to memberAccess)
    this.primary = this.RULE("primary", () => {
      this.OR([
        { ALT: () => this.SUBRULE(this.arrayLiteral) },
        { ALT: () => this.CONSUME(StringLiteral) },
        { ALT: () => this.CONSUME(NumberLiteral) },
        { ALT: () => this.CONSUME(True) },
        { ALT: () => this.CONSUME(False) },
        { ALT: () => this.CONSUME(Null) },
        { ALT: () => this.SUBRULE(this.identifierExpr) },
        {
          ALT: () => {
            this.CONSUME(LParen);
            this.SUBRULE(this.expression);
            this.CONSUME(RParen);
          },
        },
      ]);
    });

    // Identifier expression: plain identifier or function call
    this.identifierExpr = this.RULE("identifierExpr", () => {
      this.CONSUME(Identifier);
      this.OPTION(() => {
        this.CONSUME(LParen);
        this.SUBRULE(this.argumentList);
        this.CONSUME(RParen);
      });
    });

    this.methodCall = this.RULE("methodCall", () => {
      this.OR([
        { ALT: () => this.CONSUME(ArrayMethod) },
        { ALT: () => this.CONSUME(StringMethod) },
      ]);
      this.CONSUME(LParen);
      this.SUBRULE(this.argumentList);
      this.CONSUME(RParen);
    });

    this.arrayLiteral = this.RULE("arrayLiteral", () => {
      this.CONSUME(LBracket);
      this.OPTION(() => this.SUBRULE(this.valueList));
      this.CONSUME(RBracket);
    });

    this.valueList = this.RULE("valueList", () => {
      this.SUBRULE(this.literal);
      this.MANY(() => {
        this.CONSUME(Comma);
        this.SUBRULE2(this.literal);
      });
    });

    this.argumentList = this.RULE("argumentList", () => {
      this.OPTION(() => {
        this.SUBRULE(this.expression);
        this.MANY(() => {
          this.CONSUME(Comma);
          this.SUBRULE2(this.expression);
        });
      });
    });

    this.literal = this.RULE("literal", () => {
      this.OR([
        { ALT: () => this.CONSUME(StringLiteral) },
        { ALT: () => this.CONSUME(NumberLiteral) },
        { ALT: () => this.CONSUME(True) },
        { ALT: () => this.CONSUME(False) },
        { ALT: () => this.CONSUME(Null) },
      ]);
    });

    this.performSelfAnalysis();
  }

  parseExpression(expression: string) {
    const lexer = new Lexer(allTokens, { ensureOptimizations: true });
    const lexResult = lexer.tokenize(expression);

    this.input = lexResult.tokens;

    const cst = this.expression();

    return {
      cst,
      lexErrors: lexResult.errors,
      parseErrors: this.errors,
    };
  }
}

export const conditionParser = new ConditionParser();
