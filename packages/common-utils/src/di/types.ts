// ============================================================
// Core type definitions
// ============================================================

import type { Binding, BindToFluentSyntax, BindInFluentSyntax, BindWhenFluentSyntax } from "./binding.js";

/**
 * Service Identifier Types
 * Can be Symbol, string, or class constructor
 */
export type ServiceIdentifier<T = unknown> = symbol | string | (new (...args: unknown[]) => T);

/**
 * Binding Scope Types
 */
export type BindingScope = "transient" | "singleton" | "scoped";

/**
 * Binding Type
 */
export type BindingType = "instance" | "constant" | "factory" | "dynamic";

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
 * Uses a more flexible definition to accept constructors with any parameter types
 */
export type Constructor<T = object> = new (...args: any[]) => T;

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