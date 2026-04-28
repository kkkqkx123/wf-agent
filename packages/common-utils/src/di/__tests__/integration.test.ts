import { describe, it, expect, beforeEach } from "vitest";
import { Container, BindingScope } from "../index.js";

// ============================================================
// Integration testing - Verifying the integrity of the overall functionality
// ============================================================

describe("Integration testing", () => {
  let container: Container;

  beforeEach(() => {
    container = new Container();
  });

  // ============================================================
  // Scenario 1: The complete dependency chain of a web application
  // ============================================================
  describe("Web Application Dependency Chain", () => {
    const ILogger = Symbol.for("ILogger");
    const IConfig = Symbol.for("IConfig");
    const IDatabase = Symbol.for("IDatabase");
    const ICache = Symbol.for("ICache");
    const IUserRepository = Symbol.for("IUserRepository");
    const IUserService = Symbol.for("IUserService");
    const IController = Symbol.for("IController");

    interface Config {
      dbHost: string;
      dbPort: number;
      cacheTTL: number;
    }

    interface Logger {
      logs: string[];
      log(msg: string): void;
    }

    interface Database {
      query(sql: string): any[];
    }

    interface Cache {
      get(key: string): any | undefined;
      set(key: string, value: any): void;
    }

    interface UserRepository {
      findById(id: number): any | undefined;
      findAll(): any[];
    }

    interface UserService {
      getUser(id: number): any | undefined;
      getAllUsers(): any[];
    }

    interface Controller {
      handleRequest(action: string, params?: any): any;
    }

    class AppConfig implements Config {
      dbHost = "localhost";
      dbPort = 3306;
      cacheTTL = 3600;
    }

    class AppLogger implements Logger {
      logs: string[] = [];
      log(msg: string): void {
        this.logs.push(msg);
      }
    }

    class AppDatabase implements Database {
      static $inject = [ILogger, IConfig] as const;
      constructor(
        private logger: Logger,
        private config: Config,
      ) {}

      query(sql: string): any[] {
        this.logger.log(`DB Query: ${sql}`);
        return [{ id: 1, name: "Test" }];
      }
    }

    class AppCache implements Cache {
      static $inject = [ILogger, IConfig] as const;
      private store = new Map<string, any>();

      constructor(
        private logger: Logger,
        private config: Config,
      ) {}

      get(key: string): any | undefined {
        this.logger.log(`Cache Get: ${key}`);
        return this.store.get(key);
      }

      set(key: string, value: any): void {
        this.logger.log(`Cache Set: ${key}`);
        this.store.set(key, value);
      }
    }

    class AppUserRepository implements UserRepository {
      static $inject = [ILogger, IDatabase] as const;
      constructor(
        private logger: Logger,
        private db: Database,
      ) {}

      findById(id: number): any | undefined {
        this.logger.log(`Repository FindById: ${id}`);
        const results = this.db.query(`SELECT * FROM users WHERE id = ${id}`);
        return results[0];
      }

      findAll(): any[] {
        this.logger.log("Repository FindAll");
        return this.db.query("SELECT * FROM users");
      }
    }

    class AppUserService implements UserService {
      static $inject = [ILogger, IUserRepository, ICache] as const;
      constructor(
        private logger: Logger,
        private repo: UserRepository,
        private cache: Cache,
      ) {}

      getUser(id: number): any | undefined {
        this.logger.log(`Service GetUser: ${id}`);
        const cacheKey = `user:${id}`;
        let user = this.cache.get(cacheKey);
        if (!user) {
          user = this.repo.findById(id);
          if (user) {
            this.cache.set(cacheKey, user);
          }
        }
        return user;
      }

      getAllUsers(): any[] {
        this.logger.log("Service GetAllUsers");
        return this.repo.findAll();
      }
    }

    class UserController implements Controller {
      static $inject = [ILogger, IUserService] as const;
      constructor(
        private logger: Logger,
        private service: UserService,
      ) {}

      handleRequest(action: string, params?: any): any {
        this.logger.log(`Controller HandleRequest: ${action}`);
        switch (action) {
          case "getUser":
            return this.service.getUser(params?.id);
          case "getAllUsers":
            return this.service.getAllUsers();
          default:
            throw new Error(`Unknown action: ${action}`);
        }
      }
    }

    it("The complete dependency chain should be parsed successfully.", () => {
      // Configure the container
      container.bind(IConfig).toConstantValue(new AppConfig());
      container.bind(ILogger).to(AppLogger).inSingletonScope();
      container.bind(IDatabase).to(AppDatabase);
      container.bind(ICache).to(AppCache);
      container.bind(IUserRepository).to(AppUserRepository);
      container.bind(IUserService).to(AppUserService);
      container.bind(IController).to(UserController);

      // Parse Controller (Root Dependency)
      const controller = container.get<Controller>(IController);

      // Verify that the dependency chain is correctly constructed.
      expect(controller).toBeInstanceOf(UserController);

      // Perform business operations
      const user = controller.handleRequest("getUser", { id: 1 });
      expect(user).toEqual({ id: 1, name: "Test" });

      // Verify log records (to prove that all dependencies have been called correctly)
      const logger = container.get<Logger>(ILogger);
      expect(logger.logs).toContain("Controller HandleRequest: getUser");
      expect(logger.logs).toContain("Service GetUser: 1");
      expect(logger.logs).toContain("Cache Get: user:1");
      expect(logger.logs).toContain("Repository FindById: 1");
      expect(logger.logs).toContain("DB Query: SELECT * FROM users WHERE id = 1");
    });

    it("You should use a single instance Logger to log all operations", () => {
      container.bind(IConfig).toConstantValue(new AppConfig());
      container.bind(ILogger).to(AppLogger).inSingletonScope();
      container.bind(IDatabase).to(AppDatabase);
      container.bind(ICache).to(AppCache);
      container.bind(IUserRepository).to(AppUserRepository);
      container.bind(IUserService).to(AppUserService);
      container.bind(IController).to(UserController);

      const controller = container.get<Controller>(IController);
      controller.handleRequest("getUser", { id: 1 });
      controller.handleRequest("getAllUsers");

      const logger = container.get<Logger>(ILogger);
      // All operations should use the same Logger instance.
      expect(logger.logs.length).toBeGreaterThan(5);
    });
  });

  // ============================================================
  // Scenario 2: Plugin System (Multiple Bindings)
  // ============================================================
  describe("Plugin system", () => {
    const IPlugin = Symbol.for("IPlugin");
    const IPluginManager = Symbol.for("IPluginManager");

    interface Plugin {
      name: string;
      priority: number;
      execute(data: any): any;
    }

    interface PluginManager {
      executeAll(data: any): any[];
      getPluginNames(): string[];
    }

    class ValidationPlugin implements Plugin {
      name = "ValidationPlugin";
      priority = 1;
      execute(data: any): any {
        return { ...data, validated: true };
      }
    }

    class TransformPlugin implements Plugin {
      name = "TransformPlugin";
      priority = 2;
      execute(data: any): any {
        return { ...data, transformed: true };
      }
    }

    class LoggingPlugin implements Plugin {
      static $inject = [Symbol.for("ILogger")] as const;
      name = "LoggingPlugin";
      priority = 0;
      constructor(private logger: { log(msg: string): void }) {}
      execute(data: any): any {
        this.logger.log(`Processing: ${JSON.stringify(data)}`);
        return { ...data, logged: true };
      }
    }

    class DefaultPluginManager implements PluginManager {
      static $inject = [Symbol.for("IPluginArray")] as const;
      constructor(private plugins: Plugin[]) {}

      executeAll(data: any): any[] {
        const sorted = [...this.plugins].sort((a, b) => a.priority - b.priority);
        return sorted.map(p => p.execute(data));
      }

      getPluginNames(): string[] {
        return this.plugins.map(p => p.name);
      }
    }

    it("Should support multiple plugins bound to the same identifier", () => {
      const ILogger = Symbol.for("ILogger");
      const IPluginArray = Symbol.for("IPluginArray");
      const logger = {
        logs: [] as string[],
        log(msg: string) {
          this.logs.push(msg);
        },
      };

      container.bind(ILogger).toConstantValue(logger);
      container.bind(IPlugin).to(ValidationPlugin);
      container.bind(IPlugin).to(TransformPlugin);
      container.bind(IPlugin).to(LoggingPlugin);
      // Use a factory function to bind multiple plugins into an array.
      container.bind(IPluginArray).toDynamicValue(c => {
        return c.getAll<Plugin>(IPlugin);
      });
      container.bind(IPluginManager).to(DefaultPluginManager);

      const manager = container.get<PluginManager>(IPluginManager);

      expect(manager.getPluginNames()).toHaveLength(3);
      expect(manager.getPluginNames()).toContain("ValidationPlugin");
      expect(manager.getPluginNames()).toContain("TransformPlugin");
      expect(manager.getPluginNames()).toContain("LoggingPlugin");

      const results = manager.executeAll({ input: "test" });
      expect(results).toHaveLength(3);
    });
  });

  // ============================================================
  // Scenario 3: Configuration and Environment Switching
  // ============================================================
  describe("Configuration and Environment Switching", () => {
    const IConfig = Symbol.for("IConfig");
    const ILogger = Symbol.for("ILogger");
    const IEmailService = Symbol.for("IEmailService");

    interface Config {
      environment: "development" | "production";
      smtpHost: string;
      smtpPort: number;
    }

    interface Logger {
      messages: string[];
      info(msg: string): void;
    }

    interface EmailService {
      send(to: string, subject: string): void;
    }

    class DevConfig implements Config {
      environment = "development" as const;
      smtpHost = "localhost";
      smtpPort = 1025;
    }

    class ProdConfig implements Config {
      environment = "production" as const;
      smtpHost = "smtp.production.com";
      smtpPort = 587;
    }

    class ConsoleLogger implements Logger {
      messages: string[] = [];
      info(msg: string): void {
        this.messages.push(`[DEV] ${msg}`);
      }
    }

    class FileLogger implements Logger {
      static $inject = [IConfig] as const;
      messages: string[] = [];
      constructor(private config: Config) {}
      info(msg: string): void {
        this.messages.push(`[PROD][${this.config.environment}] ${msg}`);
      }
    }

    class DevEmailService implements EmailService {
      static $inject = [ILogger, IConfig] as const;
      constructor(
        private logger: Logger,
        private config: Config,
      ) {}
      send(to: string, subject: string): void {
        this.logger.info(`[MOCK EMAIL] To: ${to}, Subject: ${subject}`);
      }
    }

    class SmtpEmailService implements EmailService {
      static $inject = [ILogger, IConfig] as const;
      constructor(
        private logger: Logger,
        private config: Config,
      ) {}
      send(to: string, subject: string): void {
        this.logger.info(`[SMTP] Host: ${this.config.smtpHost}, To: ${to}, Subject: ${subject}`);
      }
    }

    it("Simulation services should be used in the development environment", () => {
      container.bind(IConfig).toConstantValue(new DevConfig());
      container.bind(ILogger).to(ConsoleLogger).inSingletonScope();
      container.bind(IEmailService).to(DevEmailService);

      const emailService = container.get<EmailService>(IEmailService);
      emailService.send("test@example.com", "Test Subject");

      const logger = container.get<Logger>(ILogger);
      expect(logger.messages[0]).toContain("[DEV]");
      expect(logger.messages[0]).toContain("[MOCK EMAIL]");
    });

    it("Conditional binding switching implementation should be supported.", () => {
      // Simulate a production environment
      container.bind(IConfig).toConstantValue(new ProdConfig());
      container
        .bind(ILogger)
        .to(FileLogger)
        .inSingletonScope()
        .when(() => true);
      container.bind(IEmailService).to(SmtpEmailService);

      const emailService = container.get<EmailService>(IEmailService);
      emailService.send("prod@example.com", "Production Email");

      const logger = container.get<Logger>(ILogger);
      expect(logger.messages[0]).toContain("[PROD]");
      expect(logger.messages[0]).toContain("[SMTP]");
    });
  });

  // ============================================================
  // Scenario 4: Lifecycle Management
  // ============================================================
  describe("life cycle management", () => {
    const IService = Symbol.for("IService");
    const IRepository = Symbol.for("IRepository");

    let instanceCount = 0;

    class Repository {
      id = ++instanceCount;
    }

    class Service {
      static $inject = [IRepository] as const;
      id = ++instanceCount;
      constructor(public repo: Repository) {}
    }

    beforeEach(() => {
      instanceCount = 0;
    });

    it("The singleton lifecycle should be properly managed.", () => {
      container.bind(IRepository).to(Repository).inSingletonScope();
      container.bind(IService).to(Service).inSingletonScope();

      const service1 = container.get<Service>(IService);
      const service2 = container.get<Service>(IService);
      const repo1 = container.get<Repository>(IRepository);
      const repo2 = container.get<Repository>(IRepository);

      // A singleton should return the same instance.
      expect(service1).toBe(service2);
      expect(repo1).toBe(repo2);
      expect(service1.repo).toBe(repo1);
      // Only two instances were created (one each for Service and Repository).
      expect(instanceCount).toBe(2);
    });

    it("Transient life cycles should be managed correctly", () => {
      container.bind(IRepository).to(Repository);
      container.bind(IService).to(Service);

      const service1 = container.get<Service>(IService);
      const service2 = container.get<Service>(IService);

      // Transient objects should return different instances.
      expect(service1).not.toBe(service2);
      expect(service1.repo).not.toBe(service2.repo);
      // Four instances were created (2 Services, with 1 Repository created for each Service).
      expect(instanceCount).toBe(4);
    });

    it("The scope lifecycle should be properly managed.", () => {
      container.bind(IRepository).to(Repository).inScopedScope();
      container.bind(IService).to(Service).inScopedScope();

      const service1 = container.get<Service>(IService);
      const service2 = container.get<Service>(IService);

      // The same instance should be returned within the scope.
      expect(service1).toBe(service2);
      expect(service1.repo).toBe(service2.repo);
      expect(instanceCount).toBe(2);

      // After clearing the scope cache, a new instance should be created.
      container.clearScopedCache();
      const service3 = container.get<Service>(IService);

      expect(service3).not.toBe(service1);
      expect(instanceCount).toBe(4);
    });

    it("The singleton should be shared throughout the container", () => {
      const IDatabase = Symbol.for("IDatabase");
      const ICache = Symbol.for("ICache");
      const IService = Symbol.for("IService");

      let dbInstanceCount = 0;
      let cacheInstanceCount = 0;

      class Database {
        id = ++dbInstanceCount;
        query() {
          return "data";
        }
      }

      class Cache {
        static $inject = [IDatabase] as const;
        id = ++cacheInstanceCount;
        constructor(public db: Database) {}
      }

      class Service {
        static $inject = [IDatabase, ICache] as const;
        constructor(
          public db: Database,
          public cache: Cache,
        ) {}
      }

      // The database is a singleton, and the cache is transient.
      container.bind(IDatabase).to(Database).inSingletonScope();
      container.bind(ICache).to(Cache);
      container.bind(IService).to(Service);

      const service1 = container.get<Service>(IService);
      const service2 = container.get<Service>(IService);

      // The two Services should share the same Database instance.
      expect(service1.db).toBe(service2.db);
      expect(service1.db.id).toBe(service2.db.id);
      expect(dbInstanceCount).toBe(1);

      // But each Service has its own Cache instance.
      expect(service1.cache).not.toBe(service2.cache);
      expect(cacheInstanceCount).toBe(2);

      // Each Cache references the same Database.
      expect(service1.cache.db).toBe(service2.cache.db);
    });

    it("Singletons should be shared between parent and child containers.", () => {
      const ISharedService = Symbol.for("ISharedService");

      class SharedService {
        id = Math.random();
      }

      container.bind(ISharedService).to(SharedService).inSingletonScope();

      const child1 = container.createChild();
      const child2 = container.createChild();

      const instance1 = container.get<SharedService>(ISharedService);
      const instance2 = child1.get<SharedService>(ISharedService);
      const instance3 = child2.get<SharedService>(ISharedService);

      // All containers should share the same singleton.
      expect(instance1).toBe(instance2);
      expect(instance2).toBe(instance3);
    });

    it("Transients should be created every time a new instance", () => {
      const ITransientService = Symbol.for("ITransientService");

      let instanceCount = 0;
      class TransientService {
        id = ++instanceCount;
      }

      // Explicitly use transient scope (although it is transient by default).
      container.bind(ITransientService).to(TransientService).inTransientScope();

      const instance1 = container.get<TransientService>(ITransientService);
      const instance2 = container.get<TransientService>(ITransientService);
      const instance3 = container.get<TransientService>(ITransientService);

      expect(instance1).not.toBe(instance2);
      expect(instance2).not.toBe(instance3);
      expect(instance1.id).not.toBe(instance2.id);
      expect(instanceCount).toBe(3);
    });

    it("Transient dependencies should only be created once in a singleton.", () => {
      const ISingletonService = Symbol.for("ISingletonService");
      const ITransientDep = Symbol.for("ITransientDep");

      let transientCount = 0;

      class TransientDep {
        id = ++transientCount;
      }

      class SingletonService {
        static $inject = [ITransientDep] as const;
        constructor(public dep: TransientDep) {}
      }

      // The singleton service depends on a transient service.
      container.bind(ITransientDep).to(TransientDep);
      container.bind(ISingletonService).to(SingletonService).inSingletonScope();

      const service1 = container.get<SingletonService>(ISingletonService);
      const service2 = container.get<SingletonService>(ISingletonService);

      // The singleton service is created only once, so the transient dependencies are also created only once.
      expect(service1).toBe(service2);
      expect(service1.dep).toBe(service2.dep);
      expect(transientCount).toBe(1);
    });

    it("Scope caching should isolate different scopes", () => {
      const IScopedService = Symbol.for("IScopedService");

      let instanceCount = 0;
      class ScopedService {
        id = ++instanceCount;
      }

      container.bind(IScopedService).to(ScopedService).inScopedScope();

      // Get multiple times within the same scope
      const instance1 = container.get<ScopedService>(IScopedService);
      const instance2 = container.get<ScopedService>(IScopedService);

      expect(instance1).toBe(instance2);
      expect(instanceCount).toBe(1);

      // Clear scope cache
      container.clearScopedCache();

      // Get the instance again, as a new one should be created.
      const instance3 = container.get<ScopedService>(IScopedService);
      expect(instance3).not.toBe(instance1);
      expect(instanceCount).toBe(2);
    });

    it("Complex dependency chains in the domain of hybrid interactions", () => {
      const ILogger = Symbol.for("ILogger");
      const IConfig = Symbol.for("IConfig");
      const IDatabase = Symbol.for("IDatabase");
      const IService = Symbol.for("IService");

      let loggerCount = 0;
      let configCount = 0;
      let dbCount = 0;
      let serviceCount = 0;

      class Logger {
        id = ++loggerCount;
        logs: string[] = [];
        log(msg: string) {
          this.logs.push(msg);
        }
      }

      class Config {
        id = ++configCount;
        value = "test-config";
      }

      class Database {
        static $inject = [ILogger, IConfig] as const;
        id = ++dbCount;
        constructor(
          public logger: Logger,
          public config: Config,
        ) {}
      }

      class Service {
        static $inject = [ILogger, IDatabase] as const;
        id = ++serviceCount;
        constructor(
          public logger: Logger,
          public db: Database,
        ) {}
      }

      // Configure the binding:
      // Logger: Singleton (globally shared)
      // - Config: Transient (every new creation)
      // Database: Scope (Sharing within the scope)
      // - Service: Transient (per new creation)
      container.bind(ILogger).to(Logger).inSingletonScope();
      container.bind(IConfig).to(Config);
      container.bind(IDatabase).to(Database).inScopedScope();
      container.bind(IService).to(Service);

      const service1 = container.get<Service>(IService);
      const service2 = container.get<Service>(IService);

      // The service is transient; it is created anew each time.
      expect(service1).not.toBe(service2);
      expect(serviceCount).toBe(2);

      // The Logger is a singleton that is shared across the application.
      expect(service1.logger).toBe(service2.logger);
      expect(loggerCount).toBe(1);

      // "Database is a scope, shared."
      expect(service1.db).toBe(service2.db);
      expect(dbCount).toBe(1);

      // The `Config` object is transient, but it is only created once within the scope of the Database.
      // Because Database is scoped singleton, its dependencies are resolved only once.
      expect(service1.db.config).toBe(service2.db.config);
      expect(configCount).toBe(1);

      // Clear scope cache
      container.clearScopedCache();

      const service3 = container.get<Service>(IService);
      expect(service3.db).not.toBe(service1.db);
      expect(dbCount).toBe(2);

      // Logger remains a singleton.
      expect(service3.logger).toBe(service1.logger);
      expect(loggerCount).toBe(1);
    });

    it("Multi-level dependencies within the scope", () => {
      const ILevel1 = Symbol.for("ILevel1");
      const ILevel2 = Symbol.for("ILevel2");
      const ILevel3 = Symbol.for("ILevel3");

      let count1 = 0,
        count2 = 0,
        count3 = 0;

      class Level3 {
        id = ++count3;
      }

      class Level2 {
        static $inject = [ILevel3] as const;
        id = ++count2;
        constructor(public level3: Level3) {}
      }

      class Level1 {
        static $inject = [ILevel2] as const;
        id = ++count1;
        constructor(public level2: Level2) {}
      }

      // All levels use scope.
      container.bind(ILevel3).to(Level3).inScopedScope();
      container.bind(ILevel2).to(Level2).inScopedScope();
      container.bind(ILevel1).to(Level1).inScopedScope();

      const level1a = container.get<Level1>(ILevel1);
      const level1b = container.get<Level1>(ILevel1);

      // All levels are shared within the scope.
      expect(level1a).toBe(level1b);
      expect(level1a.level2).toBe(level1b.level2);
      expect(level1a.level2.level3).toBe(level1b.level2.level3);
      expect(count1).toBe(1);
      expect(count2).toBe(1);
      expect(count3).toBe(1);

      // Clear the cache and then rebuild everything.
      container.clearScopedCache();
      const level1c = container.get<Level1>(ILevel1);

      expect(level1c).not.toBe(level1a);
      expect(count1).toBe(2);
      expect(count2).toBe(2);
      expect(count3).toBe(2);
    });
  });

  // ============================================================
  // Scenario 5: Parent-Child Containers
  // ============================================================
  describe("parent-child container", () => {
    const IConfig = Symbol.for("IConfig");
    const ILogger = Symbol.for("ILogger");
    const IService = Symbol.for("IService");

    class Config {
      constructor(public name: string) {}
    }

    class Logger {
      logs: string[] = [];
      log(msg: string): void {
        this.logs.push(`${this.constructor.name}: ${msg}`);
      }
    }

    class Service {
      static $inject = [ILogger, IConfig] as const;
      constructor(
        public logger: Logger,
        public config: Config,
      ) {}
    }

    it("The child container should inherit the bindings from the parent container.", () => {
      container.bind(IConfig).toConstantValue(new Config("parent"));
      container.bind(ILogger).to(Logger).inSingletonScope();

      const child = container.createChild();
      child.bind(IService).to(Service);

      const service = child.get<Service>(IService);

      expect(service.config.name).toBe("parent");
      expect(service.logger).toBeInstanceOf(Logger);
    });

    it("The child container should be able to override the parent container's bindings", () => {
      container.bind(IConfig).toConstantValue(new Config("parent"));
      container.bind(ILogger).to(Logger).inSingletonScope();

      const child = container.createChild();
      child.bind(IConfig).toConstantValue(new Config("child"));
      child.bind(IService).to(Service);

      const service = child.get<Service>(IService);

      expect(service.config.name).toBe("child");
    });

    it("The singleton of the parent container should be shared among the child containers.", () => {
      container.bind(ILogger).to(Logger).inSingletonScope();

      const child1 = container.createChild();
      const child2 = container.createChild();

      const logger1 = child1.get<Logger>(ILogger);
      const logger2 = child2.get<Logger>(ILogger);

      expect(logger1).toBe(logger2);
    });
  });

  // ============================================================
  // Scenario 6: Factories and Dynamic Values
  // ============================================================
  describe("Factory and dynamic values", () => {
    const IConfig = Symbol.for("IConfig");
    const IConnection = Symbol.for("IConnection");
    const IConnectionFactory = Symbol.for("IConnectionFactory");

    interface Config {
      connectionString: string;
      maxConnections: number;
    }

    interface Connection {
      id: number;
      query(sql: string): any[];
    }

    interface ConnectionFactory {
      createConnection(): Connection;
    }

    class DatabaseConnection implements Connection {
      static nextId = 1;
      id = DatabaseConnection.nextId++;
      constructor(private config: Config) {}
      query(sql: string): any[] {
        return [{ query: sql, connectionId: this.id }];
      }
    }

    it("Factory functions should be supported for creating instances.", () => {
      container.bind(IConfig).toConstantValue({
        connectionString: "postgres://localhost",
        maxConnections: 10,
      });

      container.bind(IConnectionFactory).toFactory(c => {
        const config = c.get<Config>(IConfig);
        return {
          createConnection: () => new DatabaseConnection(config),
        };
      });

      const factory = container.get<ConnectionFactory>(IConnectionFactory);
      const conn1 = factory.createConnection();
      const conn2 = factory.createConnection();

      expect(conn1.id).not.toBe(conn2.id);
      expect(conn1.query("SELECT 1")[0].connectionId).toBe(conn1.id);
    });

    it("Should support dynamic values based on configuration choice implementation", () => {
      const ILogger = Symbol.for("ILogger");

      class ConsoleLogger {
        type = "console";
      }

      class FileLogger {
        type = "file";
      }

      // Scenario 1: Using file logs
      container.bind(IConfig).toConstantValue({ useFileLogger: true });
      container.bind(ILogger).toDynamicValue(c => {
        const config = c.get<{ useFileLogger: boolean }>(IConfig);
        return config.useFileLogger ? new FileLogger() : new ConsoleLogger();
      });

      const logger1 = container.get<{ type: string }>(ILogger);
      expect(logger1.type).toBe("file");

      // Scenario 2: Creating a new container using console logs
      const newContainer = new Container();
      newContainer.bind(IConfig).toConstantValue({ useFileLogger: false });
      newContainer.bind(ILogger).toDynamicValue(c => {
        const config = c.get<{ useFileLogger: boolean }>(IConfig);
        return config.useFileLogger ? new FileLogger() : new ConsoleLogger();
      });

      const logger2 = newContainer.get<{ type: string }>(ILogger);
      expect(logger2.type).toBe("console");
    });
  });

  // ============================================================
  // Scenario 7: Error Handling
  // ============================================================
  describe("Error Handling", () => {
    const IServiceA = Symbol.for("IServiceA");
    const IServiceB = Symbol.for("IServiceB");

    class ServiceA {
      static $inject = [IServiceB] as const;
      constructor(public b: { name: string }) {}
    }

    class ServiceB {
      static $inject = [IServiceA] as const;
      constructor(public a: { name: string }) {}
    }

    it("Loop dependencies should be detected", () => {
      container.bind(IServiceA).to(ServiceA);
      container.bind(IServiceB).to(ServiceB);

      expect(() => {
        container.get(IServiceA);
      }).toThrow(/Circular dependency detected/);
    });

    it("The unbound services should be processed.", () => {
      expect(() => {
        container.get(Symbol("Unbound"));
      }).toThrow(/No binding found/);
    });

    it("Fuzzy bindings should be handled", () => {
      const IService = Symbol.for("IService");
      container.bind(IService).to(class A {});
      container.bind(IService).to(class B {});

      expect(() => {
        container.get(IService);
      }).toThrow(/Ambiguous bindings/);
    });
  });

  // ============================================================
  // Scenario 8: Complex Data Streams
  // ============================================================
  describe("Complex data flow", () => {
    const IValidator = Symbol.for("IValidator");
    const ITransformer = Symbol.for("ITransformer");
    const IProcessor = Symbol.for("IProcessor");

    interface Validator {
      validate(data: any): boolean;
    }

    interface Transformer {
      transform(data: any): any;
    }

    interface Processor {
      process(data: any): any;
    }

    class DataValidator implements Validator {
      static $inject = [Symbol.for("ILogger")] as const;
      constructor(private logger: { log(msg: string): void }) {}
      validate(data: any): boolean {
        this.logger.log("Validating data");
        return data != null;
      }
    }

    class DataTransformer implements Transformer {
      static $inject = [Symbol.for("ILogger")] as const;
      constructor(private logger: { log(msg: string): void }) {}
      transform(data: any): any {
        this.logger.log("Transforming data");
        return { ...data, transformed: true };
      }
    }

    class DataProcessor implements Processor {
      static $inject = [IValidator, ITransformer, Symbol.for("ILogger")] as const;
      constructor(
        private validator: Validator,
        private transformer: Transformer,
        private logger: { log(msg: string): void },
      ) {}
      process(data: any): any {
        this.logger.log("Processing data");
        if (!this.validator.validate(data)) {
          throw new Error("Validation failed");
        }
        return this.transformer.transform(data);
      }
    }

    it("Data flow should be handled correctly", () => {
      const ILogger = Symbol.for("ILogger");
      const logs: string[] = [];
      const logger = { log: (msg: string) => logs.push(msg) };

      container.bind(ILogger).toConstantValue(logger);
      container.bind(IValidator).to(DataValidator);
      container.bind(ITransformer).to(DataTransformer);
      container.bind(IProcessor).to(DataProcessor);

      const processor = container.get<Processor>(IProcessor);
      const result = processor.process({ name: "test" });

      expect(result).toEqual({ name: "test", transformed: true });
      expect(logs).toEqual(["Processing data", "Validating data", "Transforming data"]);
    });
  });
});
