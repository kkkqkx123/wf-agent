/**
 * Stream module export: Export all stream implementations uniformly.
 *
 */

// BaseFileStream
export { BaseFileStream, type BaseFileStreamOptions } from "./base-file-stream.js";

// ConsoleStream
export { ConsoleStream, createConsoleStream } from "./console-stream.js";

// FileStream
export { FileStream, createFileStream, type FileStreamOptions } from "./file-stream.js";

// RotatingFileStream
export {
  RotatingFileStream,
  createRotatingFileStream,
  type RotatingFileStreamOptions,
} from "./rotating-file-stream.js";

// AsyncStream
export { AsyncStream, createAsyncStream } from "./async-stream.js";

// Multistream
export { Multistream, createMultistream } from "./multistream.js";
