/**
 * AST Metadata Utilities
 * Provides tools for extracting and managing metadata from AST nodes
 */

import type { ASTNode, NodeMetadata } from "./ast-types.js";

/**
 * Extract all metadata from an AST tree
 * Traverses the entire tree and collects metadata from all nodes
 * @param node Root AST node
 * @returns Array of all nodes with metadata
 */
export function extractAllMetadata(node: ASTNode): Array<{ node: ASTNode; metadata: NodeMetadata }> {
  const results: Array<{ node: ASTNode; metadata: NodeMetadata }> = [];

  function traverse(n: ASTNode): void {
    if (n.metadata) {
      results.push({ node: n, metadata: n.metadata });
    }

    // Recursively traverse child nodes
    switch (n.type) {
      case "logical":
        traverse(n.left);
        traverse(n.right);
        break;
      case "not":
        traverse(n.operand);
        break;
      case "arithmetic":
        traverse(n.left);
        traverse(n.right);
        break;
      case "ternary":
        traverse(n.condition);
        traverse(n.consequent);
        traverse(n.alternate);
        break;
      case "functionCall":
        n.arguments.forEach(traverse);
        break;
      case "memberAccess":
        traverse(n.object);
        break;
      case "arrayMethodComparison":
        traverse(n.methodNode);
        break;
      // Literal and leaf nodes have no children
      default:
        break;
    }
  }

  traverse(node);
  return results;
}

/**
 * Find node at specific source location
 * @param node Root AST node
 * @param position Character position in source string
 * @returns The node containing this position, or null
 */
export function findNodeAtPosition(node: ASTNode, position: number): ASTNode | null {
  if (!node.metadata?.location) {
    return null;
  }

  const { start, end } = node.metadata.location;
  if (position >= start && position < end) {
    // This node contains the position, check children for more specific match
    let bestMatch: ASTNode | null = node;

    function checkChildren(n: ASTNode): void {
      switch (n.type) {
        case "logical":
          const leftMatch = findNodeAtPosition(n.left, position);
          const rightMatch = findNodeAtPosition(n.right, position);
          if (leftMatch) bestMatch = leftMatch;
          if (rightMatch) bestMatch = rightMatch;
          break;
        case "not":
          const operandMatch = findNodeAtPosition(n.operand, position);
          if (operandMatch) bestMatch = operandMatch;
          break;
        case "arithmetic":
          const arithLeft = findNodeAtPosition(n.left, position);
          const arithRight = findNodeAtPosition(n.right, position);
          if (arithLeft) bestMatch = arithLeft;
          if (arithRight) bestMatch = arithRight;
          break;
        case "ternary":
          const condMatch = findNodeAtPosition(n.condition, position);
          const consMatch = findNodeAtPosition(n.consequent, position);
          const altMatch = findNodeAtPosition(n.alternate, position);
          if (condMatch) bestMatch = condMatch;
          if (consMatch) bestMatch = consMatch;
          if (altMatch) bestMatch = altMatch;
          break;
        case "functionCall":
          n.arguments.forEach(arg => {
            const argMatch = findNodeAtPosition(arg, position);
            if (argMatch) bestMatch = argMatch;
          });
          break;
        case "memberAccess":
          const objMatch = findNodeAtPosition(n.object, position);
          if (objMatch) bestMatch = objMatch;
          break;
        case "arrayMethodComparison":
          const methodMatch = findNodeAtPosition(n.methodNode, position);
          if (methodMatch) bestMatch = methodMatch;
          break;
      }
    }

    checkChildren(node);
    return bestMatch;
  }

  return null;
}

/**
 * Get human-readable description of a node's location
 * @param node AST node
 * @returns Location description string
 */
export function getNodeLocationDescription(node: ASTNode): string {
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

/**
 * Extract comments from AST tree
 * @param node Root AST node
 * @returns All comments found in the tree
 */
export function extractComments(node: ASTNode): string[] {
  const comments: string[] = [];

  function traverse(n: ASTNode): void {
    if (n.metadata?.comments) {
      comments.push(...n.metadata.comments);
    }

    // Recursively traverse child nodes
    switch (n.type) {
      case "logical":
        traverse(n.left);
        traverse(n.right);
        break;
      case "not":
        traverse(n.operand);
        break;
      case "arithmetic":
        traverse(n.left);
        traverse(n.right);
        break;
      case "ternary":
        traverse(n.condition);
        traverse(n.consequent);
        traverse(n.alternate);
        break;
      case "functionCall":
        n.arguments.forEach(traverse);
        break;
      case "memberAccess":
        traverse(n.object);
        break;
      case "arrayMethodComparison":
        traverse(n.methodNode);
        break;
      default:
        break;
    }
  }

  traverse(node);
  return comments;
}

/**
 * Create metadata with source location
 * @param start Start position
 * @param end End position
 * @param line Optional line number
 * @param column Optional column number
 * @returns NodeMetadata object
 */
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

/**
 * Add comment to existing metadata
 * @param metadata Existing metadata (or undefined)
 * @param comment Comment to add
 * @returns Updated metadata
 */
export function addComment(metadata: NodeMetadata | undefined, comment: string): NodeMetadata {
  const updated: NodeMetadata = metadata || {};
  if (!updated.comments) {
    updated.comments = [];
  }
  updated.comments.push(comment);
  return updated;
}
