import { describe, it, expect, beforeEach } from "vitest";
import { ResolutionEngine } from "../resolver.js";
import { Binding, BindingType } from "../binding.js";
import { BindingScope, Request, ServiceIdentifier, Container } from "../types.js";

// Service identifier for testing
const IService = Symbol("IService");
const IDependency = Symbol("IDependency");

// Classes for testing
class Dependency {
  value = "dependency";
}

class Service {
  static $inject = [IDependency] as const;
  constructor(public dep: Dependency) {}
}

class ServiceWithMultipleDeps {
  static $inject = [IDependency, IService] as const;
  constructor(
    public dep: Dependency,
    public service: Service,
  ) {}
}

// Create a simulation container
function createMockContainer(resolveMap: Map<ServiceIdentifier, unknown> = new Map()): Container {
  return {
    get: <T>(id: ServiceIdentifier<T>): T => {
      const value = resolveMap.get(id);
      if (value === undefined) {
        throw new Error(`No binding found for ${String(id)}`);
      }
      return value as T;
    },
    getAll: <T>(id: ServiceIdentifier<T>): T[] => {
      const value = resolveMap.get(id);
      return value ? [value as T] : [];
    },
    tryGet: <T>(id: ServiceIdentifier<T>): T | undefined => {
      return resolveMap.get(id) as T | undefined;
    },
    isBound: (id: ServiceIdentifier): boolean => {
      return resolveMap.has(id);
    },
    getWithRequest: <T>(id: ServiceIdentifier<T>, request: Request): T => {
      const value = resolveMap.get(id);
      if (value === undefined) {
        throw new Error(`No binding found for ${String(id)}`);
      }
      return value as T;
    },
  };
}

describe("ResolutionEngine", () => {
  let engine: ResolutionEngine;

  beforeEach(() => {
    engine = new ResolutionEngine();
  });

  describe("activateBinding() - 常量绑定", () => {
    it("The constant value should be returned.", () => {
      const constantValue = { name: "test" };
      const binding: Binding<typeof constantValue> = {
        id: Symbol("binding"),
        serviceId: IService,
        type: BindingType.CONSTANT,
        scope: BindingScope.SINGLETON,
        constantValue,
      };
      const request: Request = { serviceId: IService, depth: 0 };
      const container = createMockContainer();

      const result = engine.activateBinding(binding, request, container);

      expect(result).toBe(constantValue);
    });
  });

  describe("activateBinding() - 工厂绑定", () => {
    it("The factory function should be called and the result should be returned.", () => {
      const factoryValue = { created: true };
      const binding: Binding<typeof factoryValue> = {
        id: Symbol("binding"),
        serviceId: IService,
        type: BindingType.FACTORY,
        scope: BindingScope.TRANSIENT,
        factory: () => factoryValue,
      };
      const request: Request = { serviceId: IService, depth: 0 };
      const container = createMockContainer();

      const result = engine.activateBinding(binding, request, container);

      expect(result).toBe(factoryValue);
    });

    it("The container context should be passed to the factory function.", () => {
      const dep = new Dependency();
      let receivedContainer: Container | null = null;

      const binding: Binding<{ dep: Dependency }> = {
        id: Symbol("binding"),
        serviceId: IService,
        type: BindingType.FACTORY,
        scope: BindingScope.TRANSIENT,
        factory: container => {
          receivedContainer = container;
          return { dep: container.get(IDependency) as Dependency };
        },
      };
      const request: Request = { serviceId: IService, depth: 0 };
      const container = createMockContainer(new Map([[IDependency, dep]]));

      const result = engine.activateBinding(binding, request, container);

      expect(receivedContainer).toBeDefined();
      expect(result.dep).toBe(dep);
    });

    it("An error should be thrown when the factory function is missing.", () => {
      const binding: Binding<{}> = {
        id: Symbol("binding"),
        serviceId: IService,
        type: BindingType.FACTORY,
        scope: BindingScope.TRANSIENT,
      };
      const request: Request = { serviceId: IService, depth: 0 };
      const container = createMockContainer();

      expect(() => {
        engine.activateBinding(binding, request, container);
      }).toThrow(/Factory binding missing factory function/);
    });
  });

  describe("activateBinding() - 动态值绑定", () => {
    it("The dynamic value function should be called and the result should be returned.", () => {
      const dynamicValue = { dynamic: true };
      const binding: Binding<typeof dynamicValue> = {
        id: Symbol("binding"),
        serviceId: IService,
        type: BindingType.DYNAMIC,
        scope: BindingScope.TRANSIENT,
        dynamicValue: () => dynamicValue,
      };
      const request: Request = { serviceId: IService, depth: 0 };
      const container = createMockContainer();

      const result = engine.activateBinding(binding, request, container);

      expect(result).toBe(dynamicValue);
    });

    it("The container context should be passed to the dynamic value function.", () => {
      const dep = new Dependency();
      let receivedContainer: Container | null = null;

      const binding: Binding<{ dep: Dependency }> = {
        id: Symbol("binding"),
        serviceId: IService,
        type: BindingType.DYNAMIC,
        scope: BindingScope.TRANSIENT,
        dynamicValue: container => {
          receivedContainer = container;
          return { dep: container.get(IDependency) as Dependency };
        },
      };
      const request: Request = { serviceId: IService, depth: 0 };
      const container = createMockContainer(new Map([[IDependency, dep]]));

      const result = engine.activateBinding(binding, request, container);

      expect(receivedContainer).toBeDefined();
      expect(result.dep).toBe(dep);
    });

    it("An error should be thrown when a dynamic value function is missing.", () => {
      const binding: Binding<{}> = {
        id: Symbol("binding"),
        serviceId: IService,
        type: BindingType.DYNAMIC,
        scope: BindingScope.TRANSIENT,
      };
      const request: Request = { serviceId: IService, depth: 0 };
      const container = createMockContainer();

      expect(() => {
        engine.activateBinding(binding, request, container);
      }).toThrow(/Dynamic binding missing dynamic value function/);
    });
  });

  describe("activateBinding() - 实例绑定", () => {
    it("A class instance should be created.", () => {
      const binding: Binding<Dependency> = {
        id: Symbol("binding"),
        serviceId: IDependency,
        type: BindingType.INSTANCE,
        scope: BindingScope.TRANSIENT,
        implementation: Dependency,
      };
      const request: Request = { serviceId: IDependency, depth: 0 };
      const container = createMockContainer();

      const result = engine.activateBinding(binding, request, container);

      expect(result).toBeInstanceOf(Dependency);
    });

    it("The constructor dependencies should be automatically injected.", () => {
      const dep = new Dependency();
      const binding: Binding<Service> = {
        id: Symbol("binding"),
        serviceId: IService,
        type: BindingType.INSTANCE,
        scope: BindingScope.TRANSIENT,
        implementation: Service,
      };

      // Create requests, IService relies on IDependency
      const request: Request = { serviceId: IService, depth: 0 };
      const container = createMockContainer(new Map([[IDependency, dep]]));

      const result = engine.activateBinding(binding, request, container);

      expect(result).toBeInstanceOf(Service);
      expect(result.dep).toBe(dep);
    });

    it("An error should be thrown when the implementation class is missing.", () => {
      const binding: Binding<{}> = {
        id: Symbol("binding"),
        serviceId: IService,
        type: BindingType.INSTANCE,
        scope: BindingScope.TRANSIENT,
      };
      const request: Request = { serviceId: IService, depth: 0 };
      const container = createMockContainer();

      expect(() => {
        engine.activateBinding(binding, request, container);
      }).toThrow(/Instance binding missing implementation class/);
    });
  });

  describe("Singleton scope", () => {
    it("Singleton instances should be cached.", () => {
      const bindingId = Symbol("binding");
      const binding: Binding<Dependency> = {
        id: bindingId,
        serviceId: IDependency,
        type: BindingType.INSTANCE,
        scope: BindingScope.SINGLETON,
        implementation: Dependency,
      };
      const request: Request = { serviceId: IDependency, depth: 0 };
      const container = createMockContainer();

      const result1 = engine.activateBinding(binding, request, container);
      const result2 = engine.activateBinding(binding, request, container);

      expect(result1).toBe(result2);
    });

    it("A singleton instance should only be created once.", () => {
      let callCount = 0;
      const bindingId = Symbol("binding");

      class CountedService {
        id = ++callCount;
      }

      const binding: Binding<CountedService> = {
        id: bindingId,
        serviceId: IService,
        type: BindingType.INSTANCE,
        scope: BindingScope.SINGLETON,
        implementation: CountedService,
      };
      const request: Request = { serviceId: IService, depth: 0 };
      const container = createMockContainer();

      engine.activateBinding(binding, request, container);
      engine.activateBinding(binding, request, container);
      engine.activateBinding(binding, request, container);

      expect(callCount).toBe(1);
    });
  });

  describe("Singleton within the scope", () => {
    it("Scope instances should be cached.", () => {
      const bindingId = Symbol("binding");
      const binding: Binding<Dependency> = {
        id: bindingId,
        serviceId: IDependency,
        type: BindingType.INSTANCE,
        scope: BindingScope.SCOPED,
        implementation: Dependency,
      };
      const request: Request = { serviceId: IDependency, depth: 0 };
      const container = createMockContainer();

      const result1 = engine.activateBinding(binding, request, container);
      const result2 = engine.activateBinding(binding, request, container);

      expect(result1).toBe(result2);
    });

    it("After clearing the scope cache, a new instance should be created.", () => {
      const bindingId = Symbol("binding");
      const binding: Binding<Dependency> = {
        id: bindingId,
        serviceId: IDependency,
        type: BindingType.INSTANCE,
        scope: BindingScope.SCOPED,
        implementation: Dependency,
      };
      const request: Request = { serviceId: IDependency, depth: 0 };
      const container = createMockContainer();

      const result1 = engine.activateBinding(binding, request, container);
      engine.clearScopedCache();
      const result2 = engine.activateBinding(binding, request, container);

      expect(result1).not.toBe(result2);
    });
  });

  describe("Transient scope", () => {
    it("Each new instance should be created separately.", () => {
      const bindingId = Symbol("binding");
      const binding: Binding<Dependency> = {
        id: bindingId,
        serviceId: IDependency,
        type: BindingType.INSTANCE,
        scope: BindingScope.TRANSIENT,
        implementation: Dependency,
      };
      const request: Request = { serviceId: IDependency, depth: 0 };
      const container = createMockContainer();

      const result1 = engine.activateBinding(binding, request, container);
      const result2 = engine.activateBinding(binding, request, container);

      expect(result1).not.toBe(result2);
    });
  });

  describe("Circular Dependency Detection", () => {
    it("Loop dependencies should be detected and an error should be thrown.", () => {
      const ServiceBId = Symbol("ServiceB");

      class ServiceA {
        static $inject = [ServiceBId] as const;
        constructor(public b: unknown) {}
      }

      const binding: Binding<ServiceA> = {
        id: Symbol("binding"),
        serviceId: IService,
        type: BindingType.INSTANCE,
        scope: BindingScope.TRANSIENT,
        implementation: ServiceA,
      };

      // Create a request chain that will lead to circular dependencies.
      // Simulation scenario: We are parsing ServiceB, and ServiceB needs to parse IService (ServiceA).
      // ServiceA also relies on ServiceB, creating a cycle: ServiceB -> ServiceA -> ServiceB
      const parentRequest: Request = {
        serviceId: ServiceBId,
        depth: 0,
      };
      const childRequest: Request = {
        serviceId: IService,
        parentContext: parentRequest,
        depth: 1,
      };

      // Using a mock container, return an object when the ServiceBId is requested.
      // Circular dependency detection should be triggered in the createInstance method.
      const container = createMockContainer(new Map([[ServiceBId, { value: "serviceB" }]]));

      // When parsing the dependency ServiceBId of ServiceA,
      // It will be found that ServiceBId is already in the parent request chain, resulting in a circular dependency error.
      expect(() => {
        engine.activateBinding(binding, childRequest, container);
      }).toThrow(/Circular dependency detected/);
    });
  });

  describe("Unknown binding type", () => {
    it("An error should be thrown for unknown binding types.", () => {
      const binding: Binding<{}> = {
        id: Symbol("binding"),
        serviceId: IService,
        type: "unknown" as BindingType,
        scope: BindingScope.TRANSIENT,
      };
      const request: Request = { serviceId: IService, depth: 0 };
      const container = createMockContainer();

      expect(() => {
        engine.activateBinding(binding, request, container);
      }).toThrow(/Unknown binding type/);
    });
  });

  describe("clearAllCaches()", () => {
    it("All caches should be cleared.", () => {
      const singletonBindingId = Symbol("singleton-binding");
      const scopedBindingId = Symbol("scoped-binding");

      const singletonBinding: Binding<Dependency> = {
        id: singletonBindingId,
        serviceId: Symbol("SingletonService"),
        type: BindingType.INSTANCE,
        scope: BindingScope.SINGLETON,
        implementation: Dependency,
      };

      const scopedBinding: Binding<Dependency> = {
        id: scopedBindingId,
        serviceId: Symbol("ScopedService"),
        type: BindingType.INSTANCE,
        scope: BindingScope.SCOPED,
        implementation: Dependency,
      };

      const request1: Request = { serviceId: singletonBinding.serviceId, depth: 0 };
      const request2: Request = { serviceId: scopedBinding.serviceId, depth: 0 };
      const container = createMockContainer();

      const singleton1 = engine.activateBinding(singletonBinding, request1, container);
      const scoped1 = engine.activateBinding(scopedBinding, request2, container);

      engine.clearAllCaches();

      const singleton2 = engine.activateBinding(singletonBinding, request1, container);
      const scoped2 = engine.activateBinding(scopedBinding, request2, container);

      expect(singleton1).not.toBe(singleton2);
      expect(scoped1).not.toBe(scoped2);
    });
  });
});
