import type { Result, Ok, Err } from "@wf-agent/types";

/**
 * Creation succeeded.
 * @param value: The successful value
 * @returns: An Ok instance
 */
export function ok<T, E = Error>(value: T): Ok<T, E> {
  return {
    _tag: "Ok",
    value,
    isOk(): this is Ok<T, E> {
      return true;
    },
    isErr(): this is Err<E> {
      return false;
    },
    unwrap() {
      return this.value;
    },
    unwrapOrElse() {
      return this.value;
    },
    andThen(fn) {
      return fn(this.value);
    },
  };
}

/**
 * Creation failed result
 * @param error Error message
 * @returns Err instance
 */
export function err<E>(error: E): Err<E> {
  return {
    _tag: "Err",
    error,
    isOk(): this is Ok<never, E> {
      return false;
    },
    isErr(): this is Err<E> {
      return true;
    },
    unwrap() {
      throw new Error(`Called unwrap on an Err: ${String(this.error)}`);
    },
    unwrapOrElse(fn) {
      return fn(this.error);
    },
    andThen() {
      return this as unknown as Err<E>;
    },
    orElse(fn) {
      return fn(this.error);
    },
  };
}

/**
 * Combine multiple Results; return success if all are successful, otherwise return the first error.
 * @param results: An array of Results
 * @returns: The combined Result
 */
export function all<T, E>(results: Result<T, E>[]): Result<T[], E> {
  const values: T[] = [];
  for (const result of results) {
    if (result.isErr()) {
      return result as unknown as Result<T[], E>;
    }
    values.push(result.value);
  }
  return ok(values);
}

/**
 * 组合多个Result，收集所有错误
 * 全部成功时返回成功，否则返回所有错误的数组
 * @param results Result数组
 * @returns 组合后的Result，错误类型为 E[]
 */
export function allWithErrors<T, E>(results: Result<T, E>[]): Result<T[], E[]> {
  const values: T[] = [];
  const errors: E[] = [];

  for (const result of results) {
    if (result.isErr()) {
      errors.push(result.error);
    } else {
      values.push(result.value);
    }
  }

  if (errors.length > 0) {
    return err(errors);
  }

  return ok(values);
}

/**
 * Combine multiple Results and return the first successful one.
 * @param results: An array of Results
 * @returns: The first successful Result
 */
export function any<T, E>(results: Result<T, E>[]): Result<T, E> {
  if (results.length === 0) {
    return err(new Error("No results provided")) as Result<T, E>;
  }

  for (const result of results) {
    if (result.isOk()) {
      return result;
    }
  }
  return results[0] as Result<T, E>;
}

/**
 * 从可能抛出异常的异步函数创建Result，支持Signal
 *
 * 说明：
 * 1. 支持标准 AbortSignal，通过 reason 属性传递中断上下文信息
 * 2. 自动捕获异常并转换为 Result
 * 3. 类型安全，确保传入的 Signal 类型符合要求
 *
 * @param fn 可能抛出异常的异步函数，接收Signal参数
 * @param signal Signal（可选），默认为 AbortSignal
 * @returns Result实例
 *
 * @example
 * // 使用标准 AbortSignal
 * const result = await tryCatchAsyncWithSignal(
 *   (signal) => fetch(url, { signal }),
 *   abortSignal
 * );
 */
export async function tryCatchAsyncWithSignal<T, S extends AbortSignal = AbortSignal>(
  fn: (signal?: S) => Promise<T>,
  signal?: S,
): Promise<Result<T, Error>> {
  try {
    const value = await fn(signal);
    return ok(value);
  } catch (error) {
    return err(error instanceof Error ? error : new Error(String(error)));
  }
}
