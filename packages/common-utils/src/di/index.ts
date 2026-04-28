// ============================================================
// Dependency injection container-entry file
// ============================================================

// export type
export {
  ServiceIdentifier,
  BindingScope,
  BindingType,
  Request,
  Injectable,
  Constructor,
  Factory,
  DynamicValue,
  Container as IContainer,
} from "./types.js";

// Export binding correlation
export {
  Binding,
  BindToFluentSyntax,
  BindInFluentSyntax,
  BindWhenFluentSyntax,
  BindingBuilder,
} from "./binding.js";

// Export parsing engine
export { ResolutionEngine, ResolutionContext } from "./resolver.js";

// export container
export { Container } from "./container.js";