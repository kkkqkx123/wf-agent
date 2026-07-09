import type { StarterMetadata, WorkflowBundle, WorkflowStarter } from "./types.js";

export abstract class BaseStarter<C extends Record<string, unknown> = Record<string, unknown>>
  implements WorkflowStarter<C>
{
  abstract readonly metadata: StarterMetadata;
  abstract assemble(config: C): WorkflowBundle;

  onBeforeAssemble?(config: C): void | Promise<void>;
  onAfterInstall?(bundle: WorkflowBundle): void | Promise<void>;
  onBeforeUninstall?(): void | Promise<void>;
  onAfterUninstall?(): void | Promise<void>;
}
