/**
 * APIFactory - An API factory class that manages the creation of all resource API instances in a centralized manner.
 *
 * Design patterns used:
 * - Factory pattern: Responsible for creating API instances in a unified manner.
 * - Singleton pattern: Ensures that there is only one instance of the factory class in the application.
 *
 */

import { WorkflowRegistryAPI } from "../../graph/resources/workflows/workflow-registry-api.js";
import { ToolRegistryAPI } from "../resources/tools/tool-registry-api.js";
import { ThreadRegistryAPI } from "../../graph/resources/threads/thread-registry-api.js";
import { ScriptRegistryAPI } from "../resources/scripts/script-registry-api.js";
import { LLMProfileRegistryAPI } from "../resources/llm/llm-profile-registry-api.js";
import { NodeRegistryAPI } from "../../graph/resources/templates/node-template-registry-api.js";
import { TriggerTemplateRegistryAPI } from "../../graph/resources/templates/trigger-template-registry-api.js";
import { UserInteractionResourceAPI } from "../../graph/resources/user-interaction/user-interaction-resource-api.js";
import { HumanRelayResourceAPI } from "../../graph/resources/human-relay/human-relay-resource-api.js";
import { EventResourceAPI } from "../resources/events/event-resource-api.js";
import { TriggerResourceAPI } from "../../graph/resources/triggers/trigger-resource-api.js";
import { VariableResourceAPI } from "../../graph/resources/variables/variable-resource-api.js";
import { MessageResourceAPI } from "../../graph/resources/messages/message-resource-api.js";
import { SkillRegistryAPI } from "../resources/skills/skill-registry-api.js";
import { APIDependencyManager } from "./sdk-dependencies.js";

/**
 * Collection of all API instances
 */
export interface AllAPIs {
  /** Workflow API */
  workflows: WorkflowRegistryAPI;
  /** Tool API */
  tools: ToolRegistryAPI;
  /** Thread API */
  threads: ThreadRegistryAPI;
  /** Script API */
  scripts: ScriptRegistryAPI;
  /** Profile API */
  profiles: LLMProfileRegistryAPI;
  /** Node Template API */
  nodeTemplates: NodeRegistryAPI;
  /** Trigger Template API */
  triggerTemplates: TriggerTemplateRegistryAPI;
  /** User Interaction API */
  userInteractions: UserInteractionResourceAPI;
  /** Human Relay API */
  humanRelay: HumanRelayResourceAPI;
  /** Event API */
  events: EventResourceAPI;
  /** Trigger API */
  triggers: TriggerResourceAPI;
  /** Variable API */
  variables: VariableResourceAPI;
  /** Message API */
  messages: MessageResourceAPI;
  /** Skill API */
  skills: SkillRegistryAPI;
}

/**
 * APIFactory class
 *
 * Usage example:
 * ```typescript
 * // Get factory instance
 * const factory = APIFactory.getInstance();
 *
 * // Create a single API
 * const workflowAPI = factory.createWorkflowAPI();
 *
 * // Create all APIs
 * const apis = factory.createAllAPIs();
 * ```
 */
export class APIFactory {
  private static instance: APIFactory;
  private apiInstances: Partial<AllAPIs> = {};
  private dependencies: APIDependencyManager = new APIDependencyManager();

  private constructor() {}

  /**
   * Obtain a singleton instance of the factory.
   */
  public static getInstance(): APIFactory {
    if (!APIFactory.instance) {
      APIFactory.instance = new APIFactory();
    }
    return APIFactory.instance;
  }

  /**
   * Reset the factory instance
   */
  public reset(): void {
    this.apiInstances = {};
  }

  /**
   * General method for creating an API instance
   * @param key: The key name of the API instance
   * @param APIConstructor: The API constructor
   * @returns: The API instance
   */
  private createAPI<T extends AllAPIs[keyof AllAPIs]>(
    key: keyof AllAPIs,
    APIConstructor: new (deps: APIDependencyManager) => T,
  ): T {
    const instance = this.apiInstances[key];
    if (!instance) {
      const newInstance = new APIConstructor(this.dependencies) as AllAPIs[keyof AllAPIs];
      Object.assign(this.apiInstances, { [key]: newInstance });
      return newInstance as T;
    }
    return instance as T;
  }

  /**
   * Create a workflow API
   * @returns WorkflowRegistryAPI instance
   */
  public createWorkflowAPI(): WorkflowRegistryAPI {
    return this.createAPI("workflows", WorkflowRegistryAPI);
  }

  /**
   * Create a tool API
   * @returns ToolRegistryAPI instance
   */
  public createToolAPI(): ToolRegistryAPI {
    return this.createAPI("tools", ToolRegistryAPI);
  }

  /**
   * Create a thread API
   * @returns ThreadRegistryAPI instance
   */
  public createThreadAPI(): ThreadRegistryAPI {
    return this.createAPI("threads", ThreadRegistryAPI);
  }

  /**
   * Create a script API
   * @returns ScriptRegistryAPI instance
   */
  public createScriptAPI(): ScriptRegistryAPI {
    return this.createAPI("scripts", ScriptRegistryAPI);
  }

  /**
   * Create the Profile API
   * @returns ProfileRegistryAPI instance
   */
  public createProfileAPI(): LLMProfileRegistryAPI {
    return this.createAPI("profiles", LLMProfileRegistryAPI);
  }

  /**
   * Create a node template API
   * @returns NodeRegistryAPI instance
   */
  public createNodeTemplateAPI(): NodeRegistryAPI {
    return this.createAPI("nodeTemplates", NodeRegistryAPI);
  }

  /**
   * Create a trigger template API
   * @returns TriggerTemplateRegistryAPI instance
   */
  public createTriggerTemplateAPI(): TriggerTemplateRegistryAPI {
    return this.createAPI("triggerTemplates", TriggerTemplateRegistryAPI);
  }

  /**
   * Create a user interaction API
   * @returns UserInteractionResourceAPI instance
   */
  public createUserInteractionAPI(): UserInteractionResourceAPI {
    return this.createAPI("userInteractions", UserInteractionResourceAPI);
  }

  /**
   * Create the Human Relay API
   * @returns HumanRelayResourceAPI instance
   */
  public createHumanRelayAPI(): HumanRelayResourceAPI {
    return this.createAPI("humanRelay", HumanRelayResourceAPI);
  }

  /**
   * Create an event API
   * @returns EventResourceAPI instance
   */
  public createEventAPI(): EventResourceAPI {
    return this.createAPI("events", EventResourceAPI);
  }

  /**
   * Create a trigger API
   * @returns TriggerResourceAPI instance
   */
  public createTriggerAPI(): TriggerResourceAPI {
    return this.createAPI("triggers", TriggerResourceAPI);
  }

  /**
   * Create a VariableAPI instance
   * @returns VariableResourceAPI instance
   */
  public createVariableAPI(): VariableResourceAPI {
    return this.createAPI("variables", VariableResourceAPI);
  }

  /**
   * Create a message API
   * @returns MessageResourceAPI instance
   */
  public createMessageAPI(): MessageResourceAPI {
    return this.createAPI("messages", MessageResourceAPI);
  }

  /**
   * Create a Skill API
   * @returns SkillRegistryAPI instance
   */
  public createSkillAPI(): SkillRegistryAPI {
    return this.createAPI("skills", SkillRegistryAPI);
  }

  /**
   * Create all API instances
   * @returns All API instances
   */
  public createAllAPIs(): AllAPIs {
    return {
      workflows: this.createWorkflowAPI(),
      tools: this.createToolAPI(),
      threads: this.createThreadAPI(),
      scripts: this.createScriptAPI(),
      profiles: this.createProfileAPI(),
      nodeTemplates: this.createNodeTemplateAPI(),
      triggerTemplates: this.createTriggerTemplateAPI(),
      userInteractions: this.createUserInteractionAPI(),
      humanRelay: this.createHumanRelayAPI(),
      events: this.createEventAPI(),
      triggers: this.createTriggerAPI(),
      variables: this.createVariableAPI(),
      messages: this.createMessageAPI(),
      skills: this.createSkillAPI(),
    };
  }
}

/**
 * Get an instance of the factory singleton.
 * Delay initialization to avoid initializing the DI container when the module is loaded.
 */
export function getAPIFactory(): APIFactory {
  return APIFactory.getInstance();
}
