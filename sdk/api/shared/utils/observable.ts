/**
 * Observable - Implementation of a reactive event stream
 * Provides a lightweight Observable interface that supports the creation and subscription to event streams.
 */

import { sdkLogger as logger } from "../../../utils/logger.js";

/**
 * Subscriber Interface
 */
export interface Subscription {
  /** Unsubscribe */
  unsubscribe(): void;
  /** Has the subscription been canceled? */
  readonly closed: boolean;
}

/**
 * Observer Interface
 */
export interface Observer<T> {
  /** Receive the next value. */
  next(value: T): void;
  /** Received an error. */
  error(error: unknown): void;
  /** Received completion notification */
  complete(): void;
}

/**
 * Observable interface
 */
export interface Observable<T> {
  /** Subscribe to Observable */
  subscribe(observer: Observer<T>): Subscription;
  /** Subscribe to Observable (Simplified Version) */
  subscribe(
    next?: (value: T) => void,
    error?: (error: unknown) => void,
    complete?: () => void,
  ): Subscription;
}

/**
 * Observable implementation class
 */
export class ObservableImpl<T> implements Observable<T> {
  private _subscribe: (observer: Observer<T>) => TeardownLogic;

  constructor(subscribe: (observer: Observer<T>) => TeardownLogic) {
    this._subscribe = subscribe;
  }

  /**
   * Subscribe to Observable
   */
  subscribe(observer: Observer<T>): Subscription;
  subscribe(
    next?: (value: T) => void,
    error?: (error: unknown) => void,
    complete?: () => void,
  ): Subscription;
  subscribe(
    observerOrNext?: Observer<T> | ((value: T) => void),
    error?: (error: unknown) => void,
    complete?: () => void,
  ): Subscription {
    const observer: Observer<T> =
      typeof observerOrNext === "function"
        ? {
            next: observerOrNext,
            error: error || (err => logger.error("Observable error", { err })),
            complete: complete || (() => {}),
          }
        : observerOrNext!;

    let unsubscribed = false;
    const teardowns: TeardownLogic[] = [];

    const safeObserver: Observer<T> = {
      next: (value: T) => {
        if (!unsubscribed) {
          try {
            if (observer.next) {
              observer.next(value);
            }
          } catch (err) {
            safeObserver.error(err);
          }
        }
      },
      error: (err: unknown) => {
        if (!unsubscribed) {
          unsubscribed = true;
          try {
            if (observer.error) {
              observer.error(err);
            } else {
              logger.error("Observable error", { error: err });
            }
          } catch (err) {
            logger.error("Error in error handler", { error: err });
          }
          this.unsubscribeAll(teardowns);
        }
      },
      complete: () => {
        if (!unsubscribed) {
          unsubscribed = true;
          try {
            if (observer.complete) {
              observer.complete();
            }
          } catch (err) {
            logger.error("Error in complete handler", { error: err });
          }
          this.unsubscribeAll(teardowns);
        }
      },
    };

    try {
      const teardown = this._subscribe(safeObserver);
      if (teardown) {
        teardowns.push(teardown);
      }
    } catch (err) {
      safeObserver.error(err);
    }

    return {
      unsubscribe: () => {
        if (!unsubscribed) {
          unsubscribed = true;
          this.unsubscribeAll(teardowns);
        }
      },
      get closed() {
        return unsubscribed;
      },
    };
  }


  /**
   * Cancel all cleanup functions
   */
  private unsubscribeAll(teardowns: TeardownLogic[]): void {
    for (const teardown of teardowns) {
      try {
        if (typeof teardown === "function") {
          teardown();
        } else if (teardown && typeof teardown.unsubscribe === "function") {
          teardown.unsubscribe();
        }
      } catch (err) {
        logger.error("Error during teardown", { error: err });
      }
    }
    teardowns.length = 0;
  }
}

/**
 * Clean up logical types
 */
type TeardownLogic = (() => void) | Subscription | undefined;

/**
 * Create an Observable that can be controlled manually
 */
export function create<T>(subscribe: (observer: Observer<T>) => TeardownLogic): Observable<T> {
  return new ObservableImpl(subscribe);
}
