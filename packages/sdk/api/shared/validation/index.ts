/**
 * Verify module unified export
 * Provide a unified access point for simplified verification tools
 */

// Functional Validator
export {
  validateRequiredFields,
  validateStringLength,
  validatePositiveNumber,
  validateObject,
  validateArray,
  validateBoolean,
  validatePattern,
  validateEnum,
} from "./validation-strategy.js";
