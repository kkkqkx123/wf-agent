import { ICstVisitor } from "chevrotain";
import type { CstNode, IToken } from "chevrotain";
import type {
  Expression,
  LiteralExpr,
  IdentifierExpr,
  CallExpr,
  ArrayLiteralExpr,
  NodeMetadata,
  BinaryOperator,
} from "./types.js";

/**
 * Visitor to convert CST to AST
 */
export class ConditionCstToAstVisitor implements ICstVisitor<CstNode, Expression | Expression[]> {
  public validateVisitor(): void {}

  public visit(ctx: any): Expression | Expression[] {
    if (Array.isArray(ctx) && ctx.length > 0) {
      return this.visit(ctx[0]);
    }

    if (ctx.name && ctx.children) {
      return this.visitCstNode(ctx);
    }

    return this.visitChildren(ctx);
  }

  private visitCstNode(node: CstNode): Expression | Expression[] {
    switch (node.name) {
      case "expression":
        return this.visitExpression(node.children);
      case "ternary":
        return this.visitTernary(node.children);
      case "logicalOr":
        return this.visitLogicalOr(node.children);
      case "logicalAnd":
        return this.visitLogicalAnd(node.children);
      case "notExpr":
        return this.visitNotExpr(node.children);
      case "comparison":
        return this.visitComparison(node.children);
      case "addition":
        return this.visitAddition(node.children);
      case "multiplication":
        return this.visitMultiplication(node.children);
      case "unary":
        return this.visitUnary(node.children);
      case "memberAccess":
        return this.visitMemberAccess(node.children);
      case "primary":
        return this.visitPrimary(node.children);
      case "methodCall":
        return this.visitMethodCall(node.children);
      case "functionCall":
        return this.visitFunctionCall(node.children);
      case "identifierExpr":
        return this.visitIdentifierExpr(node.children);
      case "arrayLiteral":
        return this.visitArrayLiteral(node.children);
      case "valueList":
        return this.visitValueList(node.children);
      case "argumentList":
        return this.visitArgumentList(node.children);
      case "literal":
        return this.visitLiteral(node.children);
      default:
        throw new Error(`Unknown CST node type: ${node.name}`);
    }
  }

  private visitChildren(ctx: any): Expression | Expression[] {
    if (
      ctx.functionCall ||
      ctx.identifierExpr ||
      ctx.arrayLiteral ||
      ctx.StringLiteral ||
      ctx.NumberLiteral ||
      ctx.True ||
      ctx.False ||
      ctx.Null ||
      ctx.Identifier ||
      ctx.expression
    ) {
      return this.visitPrimary(ctx);
    }

    throw new Error(`Unknown CST children type: ${ctx.name}`);
  }

  public visitExpression(ctx: any): Expression {
    return this.visit(ctx.ternary[0]) as Expression;
  }

  private visitTernary(ctx: any): Expression {
    if (ctx.logicalOr && !ctx.Ternary) {
      return this.visit(ctx.logicalOr[0]) as Expression;
    }

    return {
      type: "ternary",
      condition: this.visit(ctx.logicalOr[0]) as Expression,
      consequent: this.visit(ctx.consequent[0]) as Expression,
      alternate: this.visit(ctx.alternate[0]) as Expression,
      metadata: this.extractMetadata(ctx),
    };
  }

  private visitLogicalOr(ctx: any): Expression {
    let left = this.visit(ctx.logicalAnd[0]) as Expression;
    for (let i = 1; i < ctx.logicalAnd.length; i++) {
      left = {
        type: "binary",
        operator: "||",
        left: left,
        right: this.visit(ctx.logicalAnd[i]) as Expression,
        metadata: this.extractMetadata(ctx),
      };
    }
    return left;
  }

  private visitLogicalAnd(ctx: any): Expression {
    let left = this.visit(ctx.notExpr[0]) as Expression;
    for (let i = 1; i < ctx.notExpr.length; i++) {
      left = {
        type: "binary",
        operator: "&&",
        left: left,
        right: this.visit(ctx.notExpr[i]) as Expression,
        metadata: this.extractMetadata(ctx),
      };
    }
    return left;
  }

  private visitNotExpr(ctx: any): Expression {
    if (ctx.Not && ctx.Not.length > 0) {
      const operand = this.visit(ctx.notExpr[0]) as Expression;
      return {
        type: "not",
        operand,
        metadata: this.extractMetadata(ctx),
      };
    }
    return this.visit(ctx.comparison[0]) as Expression;
  }

  private visitComparison(ctx: any): Expression {
    const left = this.visit(ctx.addition[0]) as Expression;

    let operator: string | undefined;
    if (ctx.ComparisonOp && ctx.ComparisonOp.length > 0) {
      operator = ctx.ComparisonOp[0].image;
    } else if (ctx.Contains && ctx.Contains.length > 0) {
      operator = "contains";
    } else if (ctx.In && ctx.In.length > 0) {
      operator = "in";
    }

    if (!operator) {
      return left;
    }

    const right = this.visit(ctx.addition[1]) as Expression;
    return {
      type: "binary",
      operator: operator as BinaryOperator,
      left,
      right,
      metadata: this.extractMetadata(ctx),
    };
  }

  private visitAddition(ctx: any): Expression {
    let left = this.visit(ctx.multiplication[0]) as Expression;
    for (let i = 1; i < ctx.multiplication.length; i++) {
      const operator = ctx.Plus && ctx.Plus[i - 1] ? "+" : ("-" as BinaryOperator);
      left = {
        type: "binary",
        operator,
        left: left,
        right: this.visit(ctx.multiplication[i]) as Expression,
        metadata: this.extractMetadata(ctx),
      };
    }
    return left;
  }

  private visitMultiplication(ctx: any): Expression {
    let left = this.visit(ctx.unary[0]) as Expression;
    for (let i = 1; i < ctx.unary.length; i++) {
      let operator: BinaryOperator = "*";
      if (ctx.Divide && ctx.Divide[i - 1]) {
        operator = "/";
      } else if (ctx.Modulo && ctx.Modulo[i - 1]) {
        operator = "%";
      }

      left = {
        type: "binary",
        operator,
        left: left,
        right: this.visit(ctx.unary[i]) as Expression,
        metadata: this.extractMetadata(ctx),
      };
    }
    return left;
  }

  private visitUnary(ctx: any): Expression {
    if (ctx.Minus && ctx.Minus.length > 0) {
      const operand = this.visit(ctx.unary[0]) as Expression;
      return {
        type: "unaryMinus",
        operand,
        metadata: this.extractMetadata(ctx),
      };
    }
    return this.visit(ctx.memberAccess[0]) as Expression;
  }

  private visitPrimary(ctx: any): Expression {
    if (ctx.functionCall) {
      return this.visit(ctx.functionCall[0]) as Expression;
    }
    if (ctx.identifierExpr) {
      return this.visit(ctx.identifierExpr[0]) as Expression;
    }
    if (ctx.arrayLiteral) {
      return this.visit(ctx.arrayLiteral[0]) as Expression;
    }
    if (ctx.StringLiteral) {
      return this.createLiteralExpr("string", ctx.StringLiteral[0]);
    }
    if (ctx.NumberLiteral) {
      return this.createLiteralExpr("number", ctx.NumberLiteral[0]);
    }
    if (ctx.True) {
      return this.createLiteralExpr("boolean", ctx.True[0], true);
    }
    if (ctx.False) {
      return this.createLiteralExpr("boolean", ctx.False[0], false);
    }
    if (ctx.Null) {
      return this.createLiteralExpr("null", ctx.Null[0], null);
    }
    if (ctx.expression) {
      return this.visit(ctx.expression[0]) as Expression;
    }

    throw new Error("Unknown primary expression type");
  }

  private visitMemberAccess(ctx: any): Expression {
    let obj = this.visit(ctx.primary[0]) as Expression;
    let dotIdx = 0;
    let subscriptIdx = 0;

    const totalDots = ctx.Dot?.length || 0;
    const totalSubscripts = ctx.subscript?.length || 0;

    while (dotIdx < totalDots || subscriptIdx < totalSubscripts) {
      if (dotIdx >= totalDots) {
        obj = this.processSubscript(obj, ctx.subscript[subscriptIdx++]);
        continue;
      }

      if (subscriptIdx >= totalSubscripts) {
        obj = this.processDotAccess(obj, ctx.dotAccess[dotIdx]);
        dotIdx++;
        continue;
      }

      const dotPos = ctx.Dot[dotIdx].startOffset;
      const subPos = ctx.subscript[subscriptIdx].children.LBracket[0].startOffset;
      if (dotPos < subPos) {
        obj = this.processDotAccess(obj, ctx.dotAccess[dotIdx]);
        dotIdx++;
      } else {
        obj = this.processSubscript(obj, ctx.subscript[subscriptIdx++]);
      }
    }

    return obj;
  }

  private processDotAccess(obj: Expression, dotAccessNode: any): Expression {
    const children = dotAccessNode.children;

    if (children.methodCall && children.methodCall.length > 0) {
      const methodCallCtx = children.methodCall[0].children;
      const methodName = this.extractMethodName(methodCallCtx);
      const argChildren = methodCallCtx.argumentList?.[0]?.children;
      const args = argChildren ? this.visitArgumentList(argChildren) : [];

      return {
        type: "call",
        callee: {
          type: "memberAccess",
          object: obj,
          property: methodName,
          metadata: this.extractMetadata(dotAccessNode),
        },
        arguments: args,
        methodKind: this.determineMethodKind(methodName),
        metadata: this.extractMetadata(dotAccessNode),
      };
    }

    if (children.property && children.property.length > 0) {
      const propName = children.property[0].image;
      return {
        type: "memberAccess",
        object: obj,
        property: propName,
        metadata: this.extractMetadata(dotAccessNode),
      };
    }

    return obj;
  }

  private processSubscript(obj: Expression, subscriptCtx: any): Expression {
    let index: string;
    const children = subscriptCtx.children;
    if (children.NumberLiteral) {
      index = children.NumberLiteral[0].image;
    } else if (children.StringLiteral) {
      index = children.StringLiteral[0].image.slice(1, -1);
    } else if (children.Identifier) {
      index = children.Identifier[0].image;
    } else {
      return obj;
    }

    return {
      type: "memberAccess",
      object: obj,
      property: index,
      metadata: this.extractMetadata(subscriptCtx),
    };
  }

  private visitMethodCall(ctx: any): CallExpr {
    const methodName = this.extractMethodName(ctx);
    const argChildren = ctx.argumentList?.[0]?.children;
    const args = argChildren ? this.visitArgumentList(argChildren) : [];

    return {
      type: "call",
      callee: { type: "identifier", name: methodName },
      arguments: args,
      methodKind: this.determineMethodKind(methodName),
      metadata: this.extractMetadata(ctx),
    };
  }

  private visitFunctionCall(ctx: any): CallExpr {
    const functionName = ctx.Identifier[0].image;
    const argChildren = ctx.argumentList?.[0]?.children;
    const args = argChildren ? this.visitArgumentList(argChildren) : [];

    return {
      type: "call",
      callee: {
        type: "identifier",
        name: functionName,
        metadata: this.extractMetadata(ctx),
      },
      arguments: args,
      metadata: this.extractMetadata(ctx),
    };
  }

  private visitIdentifierExpr(ctx: any): Expression {
    const name = ctx.Identifier[0].image;
    if (ctx.LParen && ctx.LParen.length > 0) {
      const argChildren = ctx.argumentList?.[0]?.children;
      const args = argChildren ? this.visitArgumentList(argChildren) : [];
      return {
        type: "call",
        callee: {
          type: "identifier",
          name,
          metadata: this.extractMetadataFromToken(ctx.Identifier[0]),
        },
        arguments: args,
        metadata: this.extractMetadata(ctx),
      } as CallExpr;
    }
    return {
      type: "identifier",
      name,
      metadata: this.extractMetadataFromToken(ctx.Identifier[0]),
    } as IdentifierExpr;
  }

  private visitArrayLiteral(ctx: any): ArrayLiteralExpr {
    const elements = ctx.valueList
      ? ctx.valueList[0].children.literal?.map(
          (litCtx: any) => this.visitLiteral(litCtx.children) as LiteralExpr,
        ) || []
      : [];

    return {
      type: "arrayLiteral",
      elements,
      metadata: this.extractMetadata(ctx),
    };
  }

  private visitValueList(ctx: any): ArrayLiteralExpr {
    const elements = ctx.literal.map(
      (litCtx: any) => this.visitLiteral(litCtx.children) as LiteralExpr,
    );
    return {
      type: "arrayLiteral",
      elements,
      metadata: this.extractMetadata(ctx),
    };
  }

  private visitArgumentList(ctx: any): Expression[] {
    if (!ctx.expression || ctx.expression.length === 0) {
      return [];
    }

    const args = [this.visit(ctx.expression[0]) as Expression];
    for (let i = 1; i < ctx.expression.length; i++) {
      args.push(this.visit(ctx.expression[i]) as Expression);
    }
    return args;
  }

  private visitLiteral(ctx: any): LiteralExpr {
    if (ctx.StringLiteral) {
      return this.createLiteralExpr("string", ctx.StringLiteral[0]);
    }
    if (ctx.NumberLiteral) {
      return this.createLiteralExpr("number", ctx.NumberLiteral[0]);
    }
    if (ctx.True) {
      return this.createLiteralExpr("boolean", ctx.True[0], true);
    }
    if (ctx.False) {
      return this.createLiteralExpr("boolean", ctx.False[0], false);
    }
    if (ctx.Null) {
      return this.createLiteralExpr("null", ctx.Null[0], null);
    }

    throw new Error("Unknown literal type");
  }

  private createLiteralExpr(
    valueType: "boolean" | "number" | "string" | "null",
    token: IToken,
    parsedValue?: any,
  ): LiteralExpr {
    let value: any;

    switch (valueType) {
      case "string":
        value = this.unescapeString(token.image);
        break;
      case "number":
        value = parseFloat(token.image);
        break;
      case "boolean":
        value = parsedValue;
        break;
      case "null":
        value = null;
        break;
    }

    return {
      type: "literal",
      valueType,
      value,
      metadata: this.extractMetadataFromToken(token),
    };
  }

  private extractMethodName(ctx: any): string {
    const children = ctx.children || ctx;
    if (children.ArrayMethod && children.ArrayMethod.length > 0) {
      return children.ArrayMethod[0].image;
    }
    if (children.StringMethod && children.StringMethod.length > 0) {
      return children.StringMethod[0].image;
    }
    throw new Error("Unknown method type");
  }

  private determineMethodKind(methodName: string): "arrayMethod" | "stringMethod" {
    const arrayMethods = [
      "someEqual",
      "someContains",
      "everyEqual",
      "everyHas",
      "countWhere",
      "countWhereContains",
      "findEqual",
      "findContains",
      "has",
      "hasContains",
      "sum",
      "avg",
      "min",
      "max",
      "someGreaterThan",
      "someLessThan",
      "everyGreaterThan",
      "everyLessThan",
      "map",
      "distinct",
      "first",
      "last",
    ];

    if (arrayMethods.includes(methodName)) {
      return "arrayMethod";
    }

    const stringMethods = ["startsWith", "endsWith", "toLowerCase", "toUpperCase", "trim"];
    if (stringMethods.includes(methodName)) {
      return "stringMethod";
    }

    throw new Error(`Unknown method: ${methodName}`);
  }

  private unescapeString(image: string): string {
    const content = image.slice(1, -1);

    return content
      .replace(/\\\\/g, "\\")
      .replace(/\\'/g, "'")
      .replace(/\\"/g, '"')
      .replace(/\\n/g, "\n")
      .replace(/\\t/g, "\t")
      .replace(/\\r/g, "\r");
  }

  private extractMetadata(ctx: any): NodeMetadata | undefined {
    const tokens = this.collectTokens(ctx);
    if (tokens.length === 0) {
      return undefined;
    }

    const firstToken = tokens[0];
    const lastToken = tokens[tokens.length - 1];

    return {
      location: {
        start: firstToken?.startOffset ?? 0,
        end: lastToken?.endOffset ?? 0,
        line: firstToken?.startLine ?? 1,
        column: firstToken?.startColumn ?? 1,
      },
    };
  }

  private extractMetadataFromToken(token: IToken): NodeMetadata {
    return {
      location: {
        start: token.startOffset,
        end: token.endOffset ?? token.startOffset,
        line: token.startLine,
        column: token.startColumn,
      },
    };
  }

  private collectTokens(ctx: any): IToken[] {
    const tokens: IToken[] = [];

    const target = ctx && ctx.children && !Array.isArray(ctx) ? ctx.children : ctx;

    for (const key in target) {
      if (Array.isArray(target[key])) {
        for (const item of target[key]) {
          if (item.tokenType) {
            tokens.push(item);
          } else if (typeof item === "object" && item !== null) {
            tokens.push(...this.collectTokens(item));
          }
        }
      }
    }

    return tokens;
  }
}

export const conditionCstToAstVisitor = new ConditionCstToAstVisitor();
