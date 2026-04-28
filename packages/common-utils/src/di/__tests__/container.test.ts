import { describe, it, expect, beforeEach } from "vitest";
import { Container } from "../container.js";

// 测试用的服务标识符 - 使用 Symbol() 而不是 Symbol.for() 避免测试间干扰
const ILogger = Symbol("ILogger");
const IDatabase = Symbol("IDatabase");
const IService = Symbol("IService");
const IConfig = Symbol("IConfig");

// Classes for testing
class Logger {
  logs: string[] = [];
  log(msg: string) {
    this.logs.push(msg);
  }
}

class Database {
  static $inject = [ILogger] as const;
  constructor(private logger: Logger) {}

  query(sql: string) {
    this.logger.log(`Query: ${sql}`);
    return [];
  }
}

class Service {
  static $inject = [ILogger, IDatabase] as const;
  constructor(
    private logger: Logger,
    private db: Database,
  ) {}

  doWork() {
    this.logger.log("Working");
    this.db.query("SELECT * FROM test");
  }
}

class Config {
  value = "test-config";
}

describe("Container", () => {
  let container: Container;

  beforeEach(() => {
    container = new Container();
  });

  describe("bind()", () => {
    it("It should be possible to bind to class implementations.", () => {
      container.bind(ILogger).to(Logger);
      const logger = container.get<Logger>(ILogger);

      expect(logger).toBeInstanceOf(Logger);
    });

    it("It should be possible to bind to constant values.", () => {
      const config = { apiUrl: "http://api.test.com" };
      container.bind(IConfig).toConstantValue(config);
      const result = container.get<typeof config>(IConfig);

      expect(result).toBe(config);
      expect(result.apiUrl).toBe("http://api.test.com");
    });

    it("It should be possible to bind it to a factory function.", () => {
      let counter = 0;
      container.bind(IService).toFactory(() => {
        counter++;
        return { id: counter };
      });

      const service1 = container.get<{ id: number }>(IService);
      const service2 = container.get<{ id: number }>(IService);

      expect(service1.id).toBe(1);
      expect(service2.id).toBe(2);
    });

    it("It should be possible to bind to dynamic values.", () => {
      container.bind(ILogger).toDynamicValue(() => new Logger());
      const logger = container.get<Logger>(ILogger);

      expect(logger).toBeInstanceOf(Logger);
    });
  });

  describe("get()", () => {
    it("The bound services should be parsed.", () => {
      container.bind(ILogger).to(Logger);
      const logger = container.get<Logger>(ILogger);

      expect(logger).toBeInstanceOf(Logger);
    });

    it("Dependencies should be injected automatically.", () => {
      container.bind(ILogger).to(Logger).inSingletonScope();
      container.bind(IDatabase).to(Database);

      const db = container.get<Database>(IDatabase);

      expect(db).toBeInstanceOf(Database);
      db.query("SELECT 1");
      // Verify that dependencies are injected correctly (Logger is singleton, so logs are preserved)
      const logger = container.get<Logger>(ILogger);
      expect(logger.logs).toEqual(["Query: SELECT 1"]);
    });

    it("Multi-level dependency injection should be supported.", () => {
      container.bind(ILogger).to(Logger).inSingletonScope();
      container.bind(IDatabase).to(Database);
      container.bind(IService).to(Service);

      const service = container.get<Service>(IService);

      expect(service).toBeInstanceOf(Service);
      service.doWork();

      const logger = container.get<Logger>(ILogger);
      expect(logger.logs).toEqual(["Working", "Query: SELECT * FROM test"]);
    });

    it("An error should be thrown when it is not bound.", () => {
      expect(() => {
        container.get(Symbol("UnboundService"));
      }).toThrow(/No binding found/);
    });

    it("An error should be thrown when multiple bindings occur.", () => {
      const IService2 = Symbol("IService2");
      container.bind(IService2).to(Logger);
      container.bind(IService2).to(Database);

      expect(() => {
        container.get(IService2);
      }).toThrow(/Ambiguous bindings/);
    });
  });

  describe("getAll()", () => {
    it("All matching bindings should be returned.", () => {
      const IService2 = Symbol("IService2");
      // Binding dependencies
      container.bind(ILogger).to(Logger);
      // Bind multiple implementations to the same service identifier
      container.bind(IService2).to(Logger);
      container.bind(IService2).to(Database);

      const services = container.getAll(IService2);

      expect(services).toHaveLength(2);
      expect(services[0]).toBeInstanceOf(Logger);
      expect(services[1]).toBeInstanceOf(Database);
    });

    it("An empty array should be returned when no binding is specified.", () => {
      const services = container.getAll(Symbol("Unbound"));

      expect(services).toEqual([]);
    });
  });

  describe("tryGet()", () => {
    it("The bound service should be returned.", () => {
      container.bind(ILogger).to(Logger);
      const logger = container.tryGet<Logger>(ILogger);

      expect(logger).toBeInstanceOf(Logger);
    });

    it("When not bound, it should return `undefined`.", () => {
      const result = container.tryGet(Symbol("UnboundService"));

      expect(result).toBeUndefined();
    });
  });

  describe("Singleton scope", () => {
    it("The same instance should be returned.", () => {
      container.bind(ILogger).to(Logger).inSingletonScope();

      const logger1 = container.get<Logger>(ILogger);
      const logger2 = container.get<Logger>(ILogger);

      expect(logger1).toBe(logger2);
    });

    it("Constant values should automatically be singletons.", () => {
      const config = { value: "test" };
      container.bind(IConfig).toConstantValue(config);

      const config1 = container.get<typeof config>(IConfig);
      const config2 = container.get<typeof config>(IConfig);

      expect(config1).toBe(config2);
    });
  });

  describe("Transient scope", () => {
    it("Different instances should be returned.", () => {
      container.bind(ILogger).to(Logger).inTransientScope();

      const logger1 = container.get<Logger>(ILogger);
      const logger2 = container.get<Logger>(ILogger);

      expect(logger1).not.toBe(logger2);
    });
  });

  describe("Singleton within the scope", () => {
    it("The same instance should be returned.", () => {
      container.bind(ILogger).to(Logger).inScopedScope();

      const logger1 = container.get<Logger>(ILogger);
      const logger2 = container.get<Logger>(ILogger);

      expect(logger1).toBe(logger2);
    });

    it("A new instance should be created after clearing the cache.", () => {
      container.bind(ILogger).to(Logger).inScopedScope();

      const logger1 = container.get<Logger>(ILogger);
      container.clearScopedCache();
      const logger2 = container.get<Logger>(ILogger);

      expect(logger1).not.toBe(logger2);
    });
  });

  describe("Subcontainer", () => {
    it("It should be possible to create sub-containers.", () => {
      const child = container.createChild();

      expect(child).toBeInstanceOf(Container);
    });

    it("The child container should inherit the bindings from the parent container.", () => {
      container.bind(ILogger).to(Logger).inSingletonScope();
      const child = container.createChild();

      const logger = child.get<Logger>(ILogger);

      expect(logger).toBeInstanceOf(Logger);
    });

    it("The binding of child containers should take precedence over that of parent containers.", () => {
      class ChildLogger extends Logger {
        child = true;
      }

      container.bind(ILogger).to(Logger).inSingletonScope();
      const child = container.createChild();
      child.bind(ILogger).to(ChildLogger).inSingletonScope();

      const logger = child.get<ChildLogger>(ILogger);

      expect(logger).toBeInstanceOf(ChildLogger);
      expect((logger as ChildLogger).child).toBe(true);
    });

    it("The singleton of the parent container should be shared among the child containers.", () => {
      container.bind(ILogger).to(Logger).inSingletonScope();
      const child = container.createChild();

      const logger1 = container.get<Logger>(ILogger);
      const logger2 = child.get<Logger>(ILogger);

      expect(logger1).toBe(logger2);
    });
  });

  describe("isBound()", () => {
    it("It should return true when the service is bound.", () => {
      container.bind(ILogger).to(Logger);

      expect(container.isBound(ILogger)).toBe(true);
    });

    it("It should return false when the service is not bound.", () => {
      expect(container.isBound(Symbol("Unbound"))).toBe(false);
    });

    it("The binding of the parent container should be checked.", () => {
      container.bind(ILogger).to(Logger);
      const child = container.createChild();

      expect(child.isBound(ILogger)).toBe(true);
    });
  });

  describe("Condition Binding", () => {
    it("The binding should be selected based on certain conditions.", () => {
      class ProductionLogger extends Logger {
        env = "production";
      }
      class DevelopmentLogger extends Logger {
        env = "development";
      }

      const ILogger2 = Symbol("ILogger2");
      container
        .bind(ILogger2)
        .to(ProductionLogger)
        .inSingletonScope()
        .when(() => process.env["NODE_ENV"] === "production");
      container
        .bind(ILogger2)
        .to(DevelopmentLogger)
        .inSingletonScope()
        .when(() => process.env["NODE_ENV"] !== "production");

      const logger = container.get<DevelopmentLogger>(ILogger2);

      expect(logger).toBeInstanceOf(DevelopmentLogger);
      expect(logger.env).toBe("development");
    });
  });

  describe("Clear the cache", () => {
    it("The scope cache should be cleared.", () => {
      container.bind(ILogger).to(Logger).inScopedScope();

      const logger1 = container.get<Logger>(ILogger);
      container.clearScopedCache();
      const logger2 = container.get<Logger>(ILogger);

      expect(logger1).not.toBe(logger2);
    });

    it("All caches should be cleared.", () => {
      const ILogger2 = Symbol("ILogger2");
      // Binding dependencies
      container.bind(ILogger).to(Logger);
      container.bind(ILogger2).to(Logger).inSingletonScope();
      container.bind(IDatabase).to(Database).inScopedScope();

      container.get<Logger>(ILogger2);
      container.get<Database>(IDatabase);

      // After clearing all caches, the singleton should remain intact
      const logger1 = container.get<Logger>(ILogger2);
      container.clearAllCaches();
      const logger2 = container.get<Logger>(ILogger2);

      // Note: the single instance cache is also cleared in the current implementation
      expect(logger1).not.toBe(logger2);
    });
  });
});
