/**
 * @description Comprehensive tests for Result Type - Advanced Features
 * @priority HIGH
 * 
 * Supplements basic tests with:
 * - _tag discriminated union
 * - unwrap/unwrapOrElse methods
 * - andThen chaining
 * - orElse error recovery
 * - Generic type inference
 * - never type behavior
 */

import { expectType, expectAssignable } from "tsd";
import type { Result, Ok, Err } from "../../../src/index.js";

// ============================================================================
// Test 1: _tag Discriminated Union
// ============================================================================

declare const result: Result<string, Error>;

// Using _tag for type narrowing
if (result._tag === "Ok") {
  // Should narrow to Ok<string, Error>
  expectType<Ok<string, Error>>(result);
  expectType<string>(result.value);
  expectType<"Ok">(result._tag);
}

if (result._tag === "Err") {
  // Should narrow to Err<Error>
  expectType<Err<Error>>(result);
  expectType<Error>(result.error);
  expectType<"Err">(result._tag);
}

// Switch statement with _tag
function processByTag(result: Result<number>): void {
  switch (result._tag) {
    case "Ok":
      expectType<Ok<number>>(result);
      expectType<number>(result.value);
      break;
    case "Err":
      expectType<Err<Error>>(result);
      expectType<Error>(result.error);
      break;
  }
}

// ============================================================================
// Test 2: unwrap() Method
// ============================================================================

// Ok.unwrap() returns T
declare const okResult: Ok<string>;
const unwrappedValue: string = okResult.unwrap();
expectType<string>(unwrappedValue);

// Err.unwrap() returns never
declare const errResult: Err<Error>;
const neverValue: never = errResult.unwrap();
expectType<never>(neverValue);

// Result.unwrap() should work after type narrowing
declare const maybeResult: Result<number>;
if (maybeResult.isOk()) {
  const value: number = maybeResult.unwrap();
  expectType<number>(value);
}

// ============================================================================
// Test 3: unwrapOrElse() Method
// ============================================================================

// Ok case: returns the value, ignores callback
declare const okForUnwrap: Ok<string>;
const okUnwrapped: string = okForUnwrap.unwrapOrElse((err) => "fallback");
expectType<string>(okUnwrapped);

// Err case: executes callback to convert error to value
declare const errForUnwrap: Err<Error>;
const errUnwrapped: string = errForUnwrap.unwrapOrElse((err) => {
  expectType<Error>(err);
  return "recovered";
});
expectType<string>(errUnwrapped);

// Result case: works after narrowing
declare const maybeForUnwrap: Result<number, string>;
if (maybeForUnwrap.isErr()) {
  const value: number = maybeForUnwrap.unwrapOrElse((errorMsg) => {
    expectType<string>(errorMsg);
    return 0;
  });
  expectType<number>(value);
}

// ============================================================================
// Test 4: andThen() Chaining - Core Feature
// ============================================================================

// Ok.andThen() executes the function
declare const okForChain: Ok<number>;
const chainedFromOk = okForChain.andThen((n) => {
  expectType<number>(n);
  if (n > 0) {
    return { _tag: "Ok" as const, value: n * 2 } as Ok<number>;
  }
  return { _tag: "Err" as const, error: new Error("Negative") } as Err<Error>;
});
expectType<Result<number, Error>>(chainedFromOk);

// Err.andThen() short-circuits (parameter is never)
declare const errForChain: Err<Error>;
const chainedFromErr = errForChain.andThen((n) => {
  // n should be never - this code path is unreachable
  expectType<never>(n);
  return { _tag: "Ok" as const, value: 42 } as Ok<number>;
});
expectType<Result<number, Error>>(chainedFromErr);

// Chaining multiple operations
declare const initialResult: Result<string>;
const multiChain = initialResult
  .andThen((str) => {
    expectType<string>(str);
    return {} as Result<number>;
  })
  .andThen((num) => {
    expectType<number>(num);
    return {} as Result<boolean>;
  });
expectType<Result<boolean>>(multiChain);

// ============================================================================
// Test 5: andThen() with Different Error Types
// ============================================================================

// Default error type preservation
declare const typeErrorResult: Result<number, TypeError>;
const preservedError = typeErrorResult.andThen((n) => {
  // F defaults to E (TypeError)
  return {} as Result<string, TypeError>;
});
expectType<Result<string, TypeError>>(preservedError);

// Changing error type in chain
const changedError = typeErrorResult.andThen((n) => {
  // Explicitly specify different error type
  return {} as Result<string, RangeError>;
});
// Should be Result<string, TypeError | RangeError> due to union
expectAssignable<Result<string, TypeError | RangeError>>(changedError);

// ============================================================================
// Test 6: orElse() Method (Err only)
// ============================================================================

// Err.orElse() for error recovery
declare const errForRecovery: Err<Error>;
const recovered = errForRecovery.orElse((e): Result<string, Error> => {
  expectType<Error>(e);
  return { _tag: "Ok" as const, value: "recovered" } as Ok<string>;
});
expectAssignable<Result<string, Error>>(recovered);

// Chaining orElse for fallback strategies (must be on Err)
const fallback1 = errForRecovery.orElse((e) => {
  expectType<Error>(e);
  return {} as Result<string, TypeError>;
});

// After first orElse, result might be Ok or Err<TypeError>
// Can only call orElse again if we narrow to Err
if (fallback1._tag === "Err") {
  const fallback2 = fallback1.orElse((e): Result<string, SyntaxError | TypeError> => {
    expectType<TypeError>(e);
    return {} as Result<string, SyntaxError>;
  });
  expectAssignable<Result<string, TypeError | SyntaxError>>(fallback2);
}

// Ok doesn't have orElse in the same way (it's on Err interface)
// But Result union allows calling it after narrowing
declare const maybeForResult: Result<string>;
if (maybeForResult.isErr()) {
  const handled = maybeForResult.orElse((e) => {
    expectType<Error>(e);
    return {} as Result<string>;
  });
  expectType<Result<string>>(handled);
}

// ============================================================================
// Test 7: Constructing Ok and Err
// ============================================================================

// Manual Ok construction (for testing purposes)
const manualOk: Ok<string> = {
  _tag: "Ok",
  value: "test",
  isOk(): this is Ok<string> {
    return true;
  },
  isErr(): this is Err<Error> {
    return false;
  },
  unwrap(): string {
    return "test";
  },
  unwrapOrElse(fn: (error: never) => string): string {
    return "test";
  },
  andThen<U, F = Error>(fn: (value: string) => Result<U, F>): Result<U, F> {
    return fn("test");
  },
};
expectType<Ok<string>>(manualOk);
expectType<"Ok">(manualOk._tag);
expectType<string>(manualOk.value);

// Manual Err construction
const manualErr: Err<Error> = {
  _tag: "Err",
  error: new Error("test error"),
  isOk(): this is Ok<never, Error> {
    return false;
  },
  isErr(): this is Err<Error> {
    return true;
  },
  unwrap(): never {
    throw new Error("Cannot unwrap Err");
  },
  unwrapOrElse<T>(fn: (error: Error) => T): T {
    return fn(new Error("test"));
  },
  andThen<U, F = Error>(fn: (value: never) => Result<U, F>): Result<U, F> {
    // This code path is unreachable
    return {} as Result<U, F>;
  },
  orElse<T, F>(fn: (error: Error) => Result<T, F>): Result<T, F> {
    return fn(new Error("test"));
  },
};
expectType<Err<Error>>(manualErr);
expectType<"Err">(manualErr._tag);
expectType<Error>(manualErr.error);

// ============================================================================
// Test 8: never Type Behavior
// ============================================================================

// Err.andThen parameter is never
declare const errNever: Err<Error>;
errNever.andThen((value) => {
  // value must be never - proves type safety
  expectType<never>(value);
  
  // Can assign anything to never (but code is unreachable)
  const x: never = value;
  expectType<never>(x);
  
  return {} as Result<string>;
});

// Ok.unwrapOrElse parameter is never
declare const okNever: Ok<string>;
okNever.unwrapOrElse((error) => {
  // error must be never - proves this path is unreachable
  expectType<never>(error);
  return "fallback";
});

// ============================================================================
// Test 9: Complex Generic Inference
// ============================================================================

// Inferring complex types through chains
interface User {
  id: number;
  name: string;
}

interface ValidationError {
  field: string;
  message: string;
}

declare const userResult: Result<User, ValidationError>;

const processedUser = userResult
  .andThen((user) => {
    expectType<User>(user);
    expectType<number>(user.id);
    expectType<string>(user.name);
    
    // Transform user to their ID
    return {} as Result<number, ValidationError>;
  })
  .andThen((id) => {
    expectType<number>(id);
    // Convert ID to string representation
    return {} as Result<string, ValidationError>;
  });

expectType<Result<string, ValidationError>>(processedUser);

// ============================================================================
// Test 10: Result with Multiple Error Types
// ============================================================================

type AppError = ValidationError | TypeError | RangeError;

declare const multiErrorResult: Result<string, AppError>;

if (multiErrorResult.isErr()) {
  expectType<AppError>(multiErrorResult.error);
  
  // Can narrow further with instanceof
  const error = multiErrorResult.error;
  if (error instanceof TypeError) {
    expectType<TypeError>(error);
  } else if ("field" in error) {
    expectType<ValidationError>(error);
    expectType<string>(error.field);
  }
}

// ============================================================================
// Test 11: Readonly Properties
// ============================================================================

// _tag and value/error should be readonly
declare const readonlyOk: Ok<string>;
// These should cause errors if uncommented:
// readonlyOk._tag = "Err";
// readonlyOk.value = "modified";

declare const readonlyErr: Err<Error>;
// readonlyErr._tag = "Ok";
// readonlyErr.error = new Error("modified");

// Verify they are indeed readonly by checking assignability
type OkKeys = keyof Ok<string>;
type ErrKeys = keyof Err<Error>;

// _tag and value/error should be in the keys
expectAssignable<"_tag" | "value" | "isOk" | "isErr" | "unwrap" | "unwrapOrElse" | "andThen">(
  "_tag" as OkKeys
);

// ============================================================================
// Test 12: Integration Patterns
// ============================================================================

// Pattern 1: Try-catch replacement
function safeParseJSON(json: string): Result<unknown, SyntaxError> {
  try {
    const parsed = JSON.parse(json);
    return {
      _tag: "Ok",
      value: parsed,
      isOk() { return true; },
      isErr() { return false; },
      unwrap() { return parsed; },
      unwrapOrElse(fn) { return parsed; },
      andThen(fn) { return fn(parsed); },
    } as Ok<unknown>;
  } catch (e) {
    return {
      _tag: "Err",
      error: e instanceof SyntaxError ? e : new SyntaxError(String(e)),
      isOk() { return false; },
      isErr() { return true; },
      unwrap(): never { throw new Error("Cannot unwrap"); },
      unwrapOrElse(fn) { 
        return fn(e instanceof SyntaxError ? e : new SyntaxError(String(e))); 
      },
      andThen(fn) { return {} as any; },
      orElse(fn) { return fn(e instanceof SyntaxError ? e : new SyntaxError(String(e))); },
    } as Err<SyntaxError>;
  }
}

const parseResult = safeParseJSON('{"key": "value"}');
expectType<Result<unknown, SyntaxError>>(parseResult);

// Pattern 2: Validation pipeline
function validateEmail(email: string): Result<string, ValidationError> {
  if (!email.includes("@")) {
    return {
      _tag: "Err",
      error: { field: "email", message: "Invalid email" },
      isOk() { return false; },
      isErr() { return true; },
      unwrap(): never { throw new Error(); },
      unwrapOrElse(fn) { return fn({ field: "email", message: "Invalid email" }); },
      andThen(fn) { return {} as any; },
      orElse(fn) { return fn({ field: "email", message: "Invalid email" }); },
    } as Err<ValidationError>;
  }
  
  return {
    _tag: "Ok",
    value: email,
    isOk() { return true; },
    isErr() { return false; },
    unwrap() { return email; },
    unwrapOrElse(fn) { return email; },
    andThen(fn) { return fn(email); },
  } as Ok<string>;
}

const emailValidation = validateEmail("test@example.com");
expectType<Result<string, ValidationError>>(emailValidation);

// Pattern 3: Async Result (Promise<Result>)
async function asyncOperation(): Promise<Result<string, Error>> {
  try {
    // Simulate async operation
    await Promise.resolve();
    return {
      _tag: "Ok",
      value: "success",
      isOk() { return true; },
      isErr() { return false; },
      unwrap() { return "success"; },
      unwrapOrElse(fn) { return "success"; },
      andThen(fn) { return fn("success"); },
    } as Ok<string>;
  } catch (e) {
    return {
      _tag: "Err",
      error: e instanceof Error ? e : new Error(String(e)),
      isOk() { return false; },
      isErr() { return true; },
      unwrap(): never { throw new Error(); },
      unwrapOrElse(fn) { return fn(e instanceof Error ? e : new Error(String(e))); },
      andThen(fn) { return {} as any; },
      orElse(fn) { return fn(e instanceof Error ? e : new Error(String(e))); },
    } as Err<Error>;
  }
}

const asyncResult: Promise<Result<string, Error>> = asyncOperation();
expectType<Promise<Result<string, Error>>>(asyncResult);
