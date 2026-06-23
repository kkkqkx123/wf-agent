import {
  BindInFluentSyntax,
  BindToFluentSyntax,
  BindWhenFluentSyntax,
  Binding,
  BindingBuilder,
} from "./binding.js";
import { ResolutionEngine } from "./resolver.js";
import { Request, ServiceIdentifier } from "./types.js";

// ============================================================
// Dependency Injection Container
// ============================================================

export class Container {
  /** Binding to the registry: Service Identifier -> Binding List */
  private bindings = new Map<symbol, Binding[]>();

  /** Parsing Engine */
  private resolver: ResolutionEngine;

  /** Parent container */
  private parent?: Container;

  constructor(parent?: Container) {
    this.parent = parent;
    this.resolver = new ResolutionEngine();
  }

  // ----------------------------------------------------------
  // Service registration
  // ----------------------------------------------------------

  /**
   * Binding Service
   * @param serviceId: Service Identifier
   * @returns: Binding Configuration Syntax
   */
  bind<T>(
    serviceId: ServiceIdentifier<T>,
  ): BindToFluentSyntax<T> & BindInFluentSyntax<T> & BindWhenFluentSyntax<T> {
    return wrapBuilder(this, serviceId);
  }

  /**
   * Add a binding to the registry
   */
  addBinding(binding: Binding): void {
    const key = this.getKey(binding.serviceId);
    const list = this.bindings.get(key) || [];
    list.push(binding);
    this.bindings.set(key, list);
  }

  // ----------------------------------------------------------
  // Service Resolution
  // ----------------------------------------------------------

  /**
   * Parse a single service
   * @param serviceId Service identifier
   * @returns Service instance
   */
  get<T>(serviceId: ServiceIdentifier<T>): T {
    const request: Request = {
      serviceId,
      depth: 0,
    };

    return this.resolve<T>(serviceId, request);
  }

  /**
   * Parse a single service (with request context, used for dependency resolution)
   * @param serviceId Service identifier
   * @param request Request context
   * @returns Service instance
   */
  getWithRequest<T>(serviceId: ServiceIdentifier<T>, request: Request): T {
    return this.resolve<T>(serviceId, request);
  }

  /**
   * Parse all matching services (multiple bindings)
   * @param serviceId Service identifier
   * @returns Array of service instances
   */
  getAll<T>(serviceId: ServiceIdentifier<T>): T[] {
    const request: Request = {
      serviceId,
      depth: 0,
    };

    return this.resolveAll<T>(serviceId, request);
  }

  /**
   * Attempt to parse the service; if it does not exist, undefined is returned.
   */
  tryGet<T>(serviceId: ServiceIdentifier<T>): T | undefined {
    try {
      return this.get<T>(serviceId);
    } catch {
      return undefined;
    }
  }

  /**
   * Core parsing logic
   */
  private resolve<T>(serviceId: ServiceIdentifier<T>, request: Request): T {
    const candidates = this.findBindings(serviceId, request);

    if (candidates.length === 0) {
      // Try to parse from the parent container.
      if (this.parent) {
        return this.parent.get(serviceId);
      }
      throw new Error(`No binding found for ${this.stringifyId(serviceId)}`);
    }

    if (candidates.length > 1) {
      throw new Error(
        `Ambiguous bindings for ${this.stringifyId(serviceId)}. ` +
          `Found ${candidates.length} bindings. Use getAll() to retrieve all bindings.`,
      );
    }

    return this.resolver.activateBinding<T>(candidates[0] as Binding<T>, request, this);
  }

  /**
   * Parse all matching bindings.
   */
  private resolveAll<T>(serviceId: ServiceIdentifier<T>, request: Request): T[] {
    const candidates = this.findBindings(serviceId, request);

    if (candidates.length === 0 && this.parent) {
      return this.parent.getAll(serviceId);
    }

    return candidates.map(binding =>
      this.resolver.activateBinding<T>(binding as Binding<T>, request, this),
    );
  }

  /**
   * Find the matching bindings.
   */
  private findBindings(serviceId: ServiceIdentifier, request: Request): Binding[] {
    const key = this.getKey(serviceId);
    const list = this.bindings.get(key) || [];

    // Filter condition constraints
    return list.filter(b => !b.when || b.when(request));
  }

  // ----------------------------------------------------------
  // Container Management
  // ----------------------------------------------------------

  /**
   * Create a sub-container
   */
  createChild(): Container {
    return new Container(this);
  }

  /**
   * Check whether the service has been bound.
   */
  isBound(serviceId: ServiceIdentifier): boolean {
    const key = this.getKey(serviceId);
    const list = this.bindings.get(key);
    return (list && list.length > 0) || (this.parent?.isBound(serviceId) ?? false);
  }

  /**
   * Clear scope cache
   */
  clearScopedCache(): void {
    this.resolver.clearScopedCache();
  }

  /**
   * Clear all caches
   */
  clearAllCaches(): void {
    this.resolver.clearAllCaches();
  }

  // ----------------------------------------------------------
  // Auxiliary method
  // ----------------------------------------------------------

  /**
   * Get the key-value pair for the service identifier
   */
  private getKey(serviceId: ServiceIdentifier): symbol {
    if (typeof serviceId === "symbol") {
      return serviceId;
    }
    if (typeof serviceId === "string") {
      return Symbol.for(serviceId);
    }
    return Symbol.for(serviceId.name);
  }

  /**
   * Convert the service identifier to a string.
   */
  private stringifyId(serviceId: ServiceIdentifier): string {
    if (typeof serviceId === "symbol") return serviceId.toString();
    if (typeof serviceId === "string") return serviceId;
    return serviceId.name;
  }
}

function wrapBuilder<T>(
  container: Container,
  serviceId: ServiceIdentifier<T>,
): BindToFluentSyntax<T> & BindInFluentSyntax<T> & BindWhenFluentSyntax<T> {
  const builder = new BindingBuilder<T>(Symbol("binding"), serviceId);
  let bindingAdded = false;

  const wrappedBuilder = new Proxy(builder, {
    get(target, prop, receiver) {
      const value = Reflect.get(target, prop, receiver);

      if (typeof value === "function" && (prop === "toConstantValue" || prop === "toFactory")) {
        return (...args: unknown[]) => {
          const result = value.apply(target, args);
          if (!bindingAdded) {
            container.addBinding(target.getBinding());
            bindingAdded = true;
          }
          return result;
        };
      }

      if (typeof value === "function") {
        return (...args: unknown[]) => {
          const result = value.apply(target, args);
          if (result === target) {
            if ((prop === "to" || prop === "toDynamicValue") && !bindingAdded) {
              container.addBinding(target.getBinding());
              bindingAdded = true;
            }
            return wrappedBuilder;
          }
          return result;
        };
      }

      return value;
    },
  }) as BindToFluentSyntax<T> & BindInFluentSyntax<T> & BindWhenFluentSyntax<T>;

  const originalWhen = builder.when.bind(builder);
  const wb = wrappedBuilder as unknown as Record<
    string,
    (constraint: (request: Request) => boolean) => void
  >;
  wb["when"] = (constraint: (request: Request) => boolean) => {
    originalWhen(constraint);
    if (!bindingAdded) {
      container.addBinding(builder.getBinding());
      bindingAdded = true;
    }
  };

  return wrappedBuilder;
}
