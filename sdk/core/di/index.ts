/**
 * DI Module Export
 * Export all public APIs of the DI container
 */

// Reexport the DI types of common-utils
export {
  Container,
  ServiceIdentifier,
  BindingScope,
  BindingType,
  Injectable,
  Constructor,
  Factory,
  DynamicValue,
} from "@wf-agent/common-utils";

// Export SDK service identifier
export * as ServiceIdentifiers from "./service-identifiers.js";

// Export container configuration function
export {
  initializeContainerWithAdapters,
  getContainer,
  resetContainer,
  isContainerInitialized,
  shutdownStorageAdapters,
} from "./container-config.js";
