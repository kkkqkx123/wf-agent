// ============================================================
// Core type definitions
// ============================================================

import type { Binding, BindToFluentSyntax, BindInFluentSyntax, BindWhenFluentSyntax } from "./binding.js";
import type { ResolutionEngine } from "./resolver.js";

/**
 * Service Identifier Types
 * Can be Symbol, string, or class constructor
 */
export type ServiceIdentifier<T = unknown> = symbol | string | (new (...args: unknown[]) => T);

/**
 * Binding Scope Types
 */
export enum BindingScope {
  /** Each time it parses, a new instance is created. */
  TRANSIENT = "transient",
  /** Global Singleton */
  SINGLETON = "singleton",
  /** Singleton within the scope */
  SCOPED = "scoped",
}

/**
 * Binding Type
 */
export enum BindingType {
  /** Class instance */
  INSTANCE = "instance",
  /** Constant values */
  CONSTANT = "constant",
  /** Factory function */
  FACTORY = "factory",
  /** Dynamic values */
  DYNAMIC = "dynamic",
}

/**
 * Parse the request context.
 */
export interface Request {
  /** Service Request Identifier */
  serviceId: ServiceIdentifier;
  /** Parent request context */
  parentContext?: Request;
  /** Analysis depth */
  depth: number;
}

/**
 * Marked interface for injectable classes
 * The implementing class needs to define a static $inject property
 */
export interface Injectable {
  /** Dependency list */
  $inject?: ServiceIdentifier[];
}

/**
 * Constructor type
 */
export type Constructor<T = object> = new (...args: unknown[]) => T;

/**
 * Container Interface
 */
export interface Container {
  get<T>(serviceId: ServiceIdentifier<T>): T;
  getWithRequest<T>(serviceId: ServiceIdentifier<T>, request: Request): T;
  getAll<T>(serviceId: ServiceIdentifier<T>): T[];
  tryGet<T>(serviceId: ServiceIdentifier<T>): T | undefined;
  isBound(serviceId: ServiceIdentifier): boolean;
  createChild(): Container;
  clearScopedCache(): void;
  clearAllCaches(): void;
  bind<T>(serviceId: ServiceIdentifier<T>): BindToFluentSyntax<T> & BindInFluentSyntax<T> & BindWhenFluentSyntax<T>;
  addBinding(binding: Binding): void;
}

/**
 * Factory function type
 */
export type Factory<T> = (container: Container) => T;

/**
 * Dynamic value function type
 */
export type DynamicValue<T> = (container: Container) => T;