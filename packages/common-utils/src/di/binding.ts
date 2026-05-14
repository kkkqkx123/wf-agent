import {
  BindingScope,
  BindingType,
  Constructor,
  DynamicValue,
  Factory,
  Request,
  ServiceIdentifier,
} from "./types.js";

// Re-export types for convenience
export type { BindingType };

// ============================================================
// Binding Models
// ============================================================

/**
 * Binding Definition
 */
export interface Binding<T = unknown> {
  /** Bind unique identifiers */
  id: symbol;
  /** service identifier */
  serviceId: ServiceIdentifier<T>;
  /** Binding type */
  type: BindingType;
  /** scope (computing) */
  scope: BindingScope;
  /** implementation class */
  implementation?: Constructor<T>;
  /** constant value */
  constantValue?: T;
  /** factory function */
  factory?: Factory<T>;
  /** dynamic value function (math.) */
  dynamicValue?: DynamicValue<T>;
  /** condition */
  when?: (request: Request) => boolean;
}

/**
 * Bind to target interface
 */
export interface BindToFluentSyntax<T> {
  /**
   * Bind to class implementation
   */
  to(constructor: Constructor<T>): BindInFluentSyntax<T>;

  /**
   * Binding to Constant Values
   */
  toConstantValue(value: T): void;

  /**
   * Binding to Factory Functions
   */
  toFactory(factory: Factory<T>): void;

  /**
   * Binding to dynamic values
   */
  toDynamicValue(factory: DynamicValue<T> | DynamicValue<unknown>): BindInFluentSyntax<T>;
}

/**
 * Scope Configuration Interface
 */
export interface BindInFluentSyntax<T> {
  /**
   * Setting up as a singleton scope
   */
  inSingletonScope(): BindWhenFluentSyntax<T>;

  /**
   * Set to transient scope
   */
  inTransientScope(): BindWhenFluentSyntax<T>;

  /**
   * Setting to in-scope singleton
   */
  inScopedScope(): BindWhenFluentSyntax<T>;

  /**
   * Setting the specified scope
   */
  inScope(scope: BindingScope): BindWhenFluentSyntax<T>;
}

/**
 * Conditional Constraints Interface
 */
export interface BindWhenFluentSyntax<_TBind> {
  /**
   * Adding Conditional Constraints
   */
  when(constraint: (request: Request) => boolean): void;
}

// ============================================================
// Binding builder
// ============================================================

export class BindingBuilder<T>
  implements BindToFluentSyntax<T>, BindInFluentSyntax<T>, BindWhenFluentSyntax<T>
{
  private binding: Binding<T>;

  constructor(bindingId: symbol, serviceId: ServiceIdentifier<T>) {
    this.binding = {
      id: bindingId,
      serviceId,
      type: "instance" as BindingType,
      scope: "transient" as BindingScope,
    };
  }

  getBinding(): Binding<T> {
    return this.binding;
  }

  // BindToFluentSyntax
  to(constructor: Constructor<T>): BindInFluentSyntax<T> {
    this.binding.type = "instance" as BindingType;
    this.binding.implementation = constructor;
    return this;
  }

  toConstantValue(value: T): void {
    this.binding.type = "constant" as BindingType;
    this.binding.constantValue = value;
    this.binding.scope = "singleton" as BindingScope;
  }

  toFactory(factory: Factory<T>): void {
    this.binding.type = "factory" as BindingType;
    this.binding.factory = factory;
  }

  toDynamicValue(factory: DynamicValue<T> | DynamicValue<unknown>): BindInFluentSyntax<T> {
    this.binding.type = "dynamic" as BindingType;
    this.binding.dynamicValue = factory as DynamicValue<T>;
    return this;
  }

  // BindInFluentSyntax
  inSingletonScope(): BindWhenFluentSyntax<T> {
    this.binding.scope = "singleton" as BindingScope;
    return this;
  }

  inTransientScope(): BindWhenFluentSyntax<T> {
    this.binding.scope = "transient" as BindingScope;
    return this;
  }

  inScopedScope(): BindWhenFluentSyntax<T> {
    this.binding.scope = "scoped" as BindingScope;
    return this;
  }

  inScope(scope: BindingScope): BindWhenFluentSyntax<T> {
    this.binding.scope = scope;
    return this;
  }

  // BindWhenFluentSyntax
  when(constraint: (request: Request) => boolean): void {
    this.binding.when = constraint;
  }
}
