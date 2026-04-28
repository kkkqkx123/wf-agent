/**
 * Result Type - Functional Error Handling
 * Provides a type-safe error handling mechanism to avoid throwing exceptions.
 */

/**
 * Result type - represents the result of an operation
 * @template T Value type on success
 * @template E Error type on failure
 */
export type Result<T, E = Error> = Ok<T> | Err<E>;

/**
 * Successful results
 */
export interface Ok<T, E = Error> {
  readonly _tag: "Ok";
  readonly value: T;
  isOk(): this is Ok<T, E>;
  isErr(): this is Err<E>;
  unwrap(): T;
  unwrapOrElse(fn: (error: never) => T): T;
  andThen<U>(fn: (value: T) => Result<U, E>): Result<U, E>;
}

/**
 * Failure results
 */
export interface Err<E> {
  readonly _tag: "Err";
  readonly error: E;
  isOk(): this is Ok<never, E>;
  isErr(): this is Err<E>;
  unwrap(): never;
  unwrapOrElse<T>(fn: (error: E) => T): T;
  andThen<U>(fn: (value: never) => Result<U, E>): Result<U, E>;
  orElse<T, F>(fn: (error: E) => Result<T, F>): Result<T, F>;
}
