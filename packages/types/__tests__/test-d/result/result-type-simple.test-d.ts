/**
 * @description Simplified tests for Result Type
 * @priority HIGH
 * 
 * Focus on basic type safety without complex generic chaining
 */

import { expectType, expectAssignable } from "tsd";
import type { Result } from "../../../src/index.js";

// ============================================================================
// Test 1: Basic Result construction
// ============================================================================

// Result with default error type
declare const stringResult: Result<string>;
expectAssignable<Result<string>>(stringResult);

// Result with custom error type  
declare const customResult: Result<number, string>;
expectAssignable<Result<number, string>>(customResult);

// ============================================================================
// Test 2: Result in function signatures
// ============================================================================

// Function returning Result
function parseNumber(input: string): Result<number, string> {
  const num = Number(input);
  if (isNaN(num)) {
    return {} as Result<number, string>;
  }
  return {} as Result<number, string>;
}

const parsed = parseNumber("42");
expectType<Result<number, string>>(parsed);

// ============================================================================
// Test 3: Type narrowing with isOk/isErr
// ============================================================================

function handleResult(result: Result<string>): void {
  if (result.isOk()) {
    // After isOk check, should be able to access value
    const value: string = result.value;
    expectType<string>(value);
  } else {
    // After isErr check, should be able to access error
    const error: Error = result.error;
    expectType<Error>(error);
  }
}

// ============================================================================
// Test 4: Custom error types
// ============================================================================

interface AppError {
  code: number;
  message: string;
}

declare const appResult: Result<string, AppError>;

if (appResult.isErr()) {
  expectType<AppError>(appResult.error);
  expectType<number>(appResult.error.code);
  expectType<string>(appResult.error.message);
}

// ============================================================================
// Test 5: Result with union types
// ============================================================================

declare const unionResult: Result<string | number>;
expectAssignable<Result<string | number>>(unionResult);

if (unionResult.isOk()) {
  const value: string | number = unionResult.value;
  expectType<string | number>(value);
}

// ============================================================================
// Test 6: Void Result
// ============================================================================

declare const voidResult: Result<void>;
expectAssignable<Result<void>>(voidResult);

if (voidResult.isOk()) {
  expectType<void>(voidResult.value);
}

// ============================================================================
// Test 7: Nested Results
// ============================================================================

declare const nestedResult: Result<Result<string>>;
expectAssignable<Result<Result<string>>>(nestedResult);

if (nestedResult.isOk()) {
  const inner: Result<string> = nestedResult.value;
  expectType<Result<string>>(inner);
}

// ============================================================================
// Test 8: Result arrays
// ============================================================================

declare const resultsArray: Array<Result<string>>;
expectType<Array<Result<string>>>(resultsArray);

// Process array of results
for (const result of resultsArray) {
  if (result.isOk()) {
    expectType<string>(result.value);
  }
}
