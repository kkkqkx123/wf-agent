/**
 * Runtime Node Configuration Exports
 * 
 * These configurations are generated during graph preprocessing and used only at runtime.
 * They should NOT be defined in static workflow TOML files.
 */

export {
  SubgraphStartNodeConfig,
  SubgraphStartVariableInput,
  SubgraphStartMessageInput,
  SubgraphEndNodeConfig,
  SubgraphEndVariableOutput,
  SubgraphEndMessageOutput,
} from './subgraph-runtime-configs.js';
