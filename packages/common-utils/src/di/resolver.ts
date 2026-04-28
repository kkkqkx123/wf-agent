import { Binding, BindingType } from "./binding.js";
import { BindingScope, Container, Constructor, Request, ServiceIdentifier } from "./types.js";

// ============================================================
// parsing engine
// ============================================================

/**
 * Analyzing Context
 */
export interface ResolutionContext {
  /** current request */
  request: Request;
  /** resolved dependency */
  resolved: Map<symbol, unknown>;
}

/**
 * parsing engine
 */
export class ResolutionEngine {
  private singletonCache = new Map<symbol, unknown>();
  private scopedCache = new Map<symbol, unknown>();

  /**
   * Activating bindings (creating instances)
   */
  activateBinding<T>(
    binding: Binding<T>,
    request: Request,
    container: Container,
  ): T {
    // Checking the singleton cache
    if (binding.scope === BindingScope.SINGLETON) {
      if (this.singletonCache.has(binding.id)) {
        return this.singletonCache.get(binding.id) as T;
      }
    }

    // Checking the Scope Cache
    if (binding.scope === BindingScope.SCOPED) {
      if (this.scopedCache.has(binding.id)) {
        return this.scopedCache.get(binding.id) as T;
      }
    }

    // Creating Instances
    let instance: T;

    switch (binding.type) {
      case BindingType.CONSTANT:
        instance = binding.constantValue as T;
        break;

      case BindingType.FACTORY:
        if (!binding.factory) {
          throw new Error("Factory binding missing factory function");
        }
        instance = binding.factory(container);
        break;

      case BindingType.DYNAMIC:
        if (!binding.dynamicValue) {
          throw new Error("Dynamic binding missing dynamic value function");
        }
        instance = binding.dynamicValue(container);
        break;

      case BindingType.INSTANCE:
        if (!binding.implementation) {
          throw new Error("Instance binding missing implementation class");
        }
        instance = this.createInstance(binding.implementation, request, (id, req) =>
          container.getWithRequest(id as ServiceIdentifier<T>, req),
        );
        break;

      default:
        throw new Error(`Unknown binding type: ${binding.type}`);
    }

    // Caching singleton/scope instances
    if (binding.scope === BindingScope.SINGLETON) {
      this.singletonCache.set(binding.id, instance);
    } else if (binding.scope === BindingScope.SCOPED) {
      this.scopedCache.set(binding.id, instance);
    }

    return instance;
  }

  /**
   * Creating class instances (automatic parsing of constructor parameters)
   */
  private createInstance<T>(
    constructor: Constructor<T>,
    request: Request,
    resolveDependency: (id: ServiceIdentifier, req: Request) => unknown,
  ): T {
    // Getting the type of a constructor parameter
    const paramTypes = this.getConstructorParams(constructor);

    // Recursively resolving dependencies
    const args = paramTypes.map(type => {
      // Detecting circular dependencies (checking the parent chain before creating a child request)
      if (this.detectCircularDependency(type, request)) {
        throw new Error(
          `Circular dependency detected: ${this.stringifyId(type)} at depth ${request.depth + 1}`,
        );
      }

      const childRequest: Request = {
        serviceId: type,
        parentContext: request,
        depth: request.depth + 1,
      };

      return resolveDependency(type, childRequest);
    });

    return new constructor(...args);
  }

  /**
   * Getting a list of constructor arguments
   */
  private getConstructorParams(constructor: Constructor<unknown>): ServiceIdentifier[] {
    // Get from static $inject property
    return (constructor as { $inject?: ServiceIdentifier[] }).$inject || [];
  }

  /**
   * Detecting cyclic dependencies
   */
  private detectCircularDependency(serviceId: ServiceIdentifier, request: Request): boolean {
    let current: Request | undefined = request;
    while (current) {
      if (current.serviceId === serviceId) {
        return true;
      }
      current = current.parentContext;
    }
    return false;
  }

  /**
   * Converting service identifiers to strings
   */
  private stringifyId(serviceId: ServiceIdentifier): string {
    if (typeof serviceId === "symbol") return serviceId.toString();
    if (typeof serviceId === "string") return serviceId;
    return serviceId.name;
  }

  /**
   * Clearing the Scope Cache
   */
  clearScopedCache(): void {
    this.scopedCache.clear();
  }

  /**
   * Clear all caches
   */
  clearAllCaches(): void {
    this.singletonCache.clear();
    this.scopedCache.clear();
  }
}
