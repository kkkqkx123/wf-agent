import type { WorkflowStarter, WorkflowBundle, StarterRegistries } from "./types.js";

export class StarterRegistry {
  private starters = new Map<string, WorkflowStarter>();
  private activeBundles = new Map<string, WorkflowBundle>();

  register(starter: WorkflowStarter): void {
    const id = starter.metadata.id;
    if (this.starters.has(id)) {
      throw new Error(`Starter "${id}" is already registered`);
    }
    this.starters.set(id, starter);
  }

  unregister(id: string): void {
    this.starters.delete(id);
  }

  get(id: string): WorkflowStarter | undefined {
    return this.starters.get(id);
  }

  list(): WorkflowStarter[] {
    return Array.from(this.starters.values());
  }

  async activate(
    id: string,
    config: Record<string, unknown>,
    registries: StarterRegistries,
  ): Promise<WorkflowBundle> {
    const starter = this.starters.get(id);
    if (!starter) {
      throw new Error(`Starter "${id}" not found`);
    }

    await starter.onBeforeAssemble?.(config);

    const bundle = starter.assemble(config) as WorkflowBundle;

    await registries.workflowRegistry.register(bundle.workflow);

    if (bundle.agentLoops) {
      for (const loop of bundle.agentLoops) {
        await registries.agentLoopRegistry.register(loop);
      }
    }

    if (bundle.nodeTemplates) {
      for (const nt of bundle.nodeTemplates) {
        await registries.nodeTemplateRegistry.register(nt);
      }
    }

    if (bundle.triggerTemplates) {
      for (const tt of bundle.triggerTemplates) {
        await registries.triggerTemplateRegistry.register(tt);
      }
    }

    if (bundle.hookTemplates) {
      for (const ht of bundle.hookTemplates) {
        await registries.hookTemplateRegistry.register(ht);
      }
    }

    if (bundle.promptTemplates) {
      for (const pt of bundle.promptTemplates) {
        await registries.promptTemplateRegistry.register(pt.id, pt);
      }
    }

    await starter.onAfterInstall?.(bundle);

    this.activeBundles.set(id, bundle);
    return bundle;
  }

  async deactivate(id: string, registries: StarterRegistries): Promise<void> {
    const starter = this.starters.get(id);
    if (!starter) return;

    await starter.onBeforeUninstall?.();

    const bundle = this.activeBundles.get(id);
    if (bundle) {
      await registries.workflowRegistry.unregister(bundle.workflow.id);

      if (bundle.agentLoops) {
        for (const loop of bundle.agentLoops) {
          await registries.agentLoopRegistry.unregister(loop.id);
        }
      }

      if (bundle.nodeTemplates) {
        for (const nt of bundle.nodeTemplates) {
          await registries.nodeTemplateRegistry.unregister(nt.name);
        }
      }

      if (bundle.triggerTemplates) {
        for (const tt of bundle.triggerTemplates) {
          await registries.triggerTemplateRegistry.unregister(tt.name);
        }
      }

      if (bundle.hookTemplates) {
        for (const ht of bundle.hookTemplates) {
          await registries.hookTemplateRegistry.unregister(ht.name);
        }
      }

      if (bundle.promptTemplates) {
        for (const pt of bundle.promptTemplates) {
          await registries.promptTemplateRegistry.unregister(pt.id);
        }
      }

      this.activeBundles.delete(id);
    }

    await starter.onAfterUninstall?.();
  }
}