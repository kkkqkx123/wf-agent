/**
 * AST Metadata Utilities
 * Provides tools for extracting and managing metadata from AST nodes
 */

import type { Expression, NodeMetadata } from "./dsl/types.js";

export function extractAllMetadata(node: Expression): Array<{ node: Expression; metadata: NodeMetadata }> {
  const results: Array<{ node: Expression; metadata: NodeMetadata }> = [];

  function traverse(n: Expression): void {
    if (n.metadata) {
      results.push({ node: n, metadata: n.metadata });
    }

    switch (n.type) {
      case "binary":
        traverse(n.left);
        traverse(n.right);
        break;
      case "not":
        traverse(n.operand);
        break;
      case "unaryMinus":
        traverse(n.operand);
        break;
      case "ternary":
        traverse(n.condition);
        traverse(n.consequent);
        traverse(n.alternate);
        break;
      case "call":
        traverse(n.callee);
        n.arguments.forEach(traverse);
        break;
      case "memberAccess":
        traverse(n.object);
        break;
      case "arrayLiteral":
        n.elements.forEach(traverse);
        break;
      default:
        break;
    }
  }

  traverse(node);
  return results;
}

export function findNodeAtPosition(node: Expression, position: number): Expression | null {
  if (!node.metadata?.location) {
    return null;
  }

  const { start, end } = node.metadata.location;
  if (position >= start && position < end) {
    let bestMatch: Expression | null = node;

    function checkChildren(n: Expression): void {
      switch (n.type) {
        case "binary":
          const leftMatch = findNodeAtPosition(n.left, position);
          const rightMatch = findNodeAtPosition(n.right, position);
          if (leftMatch) bestMatch = leftMatch;
          if (rightMatch) bestMatch = rightMatch;
          break;
        case "not":
          const operandMatch = findNodeAtPosition(n.operand, position);
          if (operandMatch) bestMatch = operandMatch;
          break;
        case "unaryMinus":
          const umMatch = findNodeAtPosition(n.operand, position);
          if (umMatch) bestMatch = umMatch;
          break;
        case "ternary":
          const condMatch = findNodeAtPosition(n.condition, position);
          const consMatch = findNodeAtPosition(n.consequent, position);
          const altMatch = findNodeAtPosition(n.alternate, position);
          if (condMatch) bestMatch = condMatch;
          if (consMatch) bestMatch = consMatch;
          if (altMatch) bestMatch = altMatch;
          break;
        case "call":
          const calleeMatch = findNodeAtPosition(n.callee, position);
          if (calleeMatch) bestMatch = calleeMatch;
          n.arguments.forEach((arg) => {
            const argMatch = findNodeAtPosition(arg, position);
            if (argMatch) bestMatch = argMatch;
          });
          break;
        case "memberAccess":
          const objMatch = findNodeAtPosition(n.object, position);
          if (objMatch) bestMatch = objMatch;
          break;
        case "arrayLiteral":
          n.elements.forEach((el) => {
            const elMatch = findNodeAtPosition(el, position);
            if (elMatch) bestMatch = elMatch;
          });
          break;
        default:
          break;
      }
    }

    checkChildren(node);
    return bestMatch;
  }

  return null;
}

export function getNodeLocationDescription(node: Expression): string {
  if (!node.metadata?.location) {
    return "Unknown location";
  }

  const { start, end, line, column } = node.metadata.location;
  const parts: string[] = [];

  if (line !== undefined && column !== undefined) {
    parts.push(`Line ${line}, Column ${column}`);
  }

  parts.push(`Position ${start}-${end}`);

  return parts.join(" (");
}

export function extractComments(node: Expression): string[] {
  const comments: string[] = [];

  function traverse(n: Expression): void {
    if (n.metadata?.comments) {
      comments.push(...n.metadata.comments);
    }

    switch (n.type) {
      case "binary":
        traverse(n.left);
        traverse(n.right);
        break;
      case "not":
        traverse(n.operand);
        break;
      case "unaryMinus":
        traverse(n.operand);
        break;
      case "ternary":
        traverse(n.condition);
        traverse(n.consequent);
        traverse(n.alternate);
        break;
      case "call":
        traverse(n.callee);
        n.arguments.forEach(traverse);
        break;
      case "memberAccess":
        traverse(n.object);
        break;
      case "arrayLiteral":
        n.elements.forEach(traverse);
        break;
      default:
        break;
    }
  }

  traverse(node);
  return comments;
}

export function createMetadata(
  start: number,
  end: number,
  line?: number,
  column?: number,
): NodeMetadata {
  return {
    location: {
      start,
      end,
      line,
      column,
    },
  };
}

export function addComment(metadata: NodeMetadata | undefined, comment: string): NodeMetadata {
  const updated: NodeMetadata = metadata || {};
  if (!updated.comments) {
    updated.comments = [];
  }
  updated.comments.push(comment);
  return updated;
}