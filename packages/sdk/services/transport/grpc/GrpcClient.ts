/**
 * gRPC Client
 *
 * Universal gRPC client based on @grpc/grpc-js
 * Supports runtime dynamic proto loading
 */

import * as grpc from "@grpc/grpc-js";
import * as protoLoader from "@grpc/proto-loader";
import { EventEmitter } from "events";
import type { GrpcClientOptions, GrpcClientState } from "./types.js";
import { GrpcHealthCheck } from "./GrpcHealthCheck.js";

export class GrpcClient extends EventEmitter {
  private options: GrpcClientOptions;
  private client: Record<string, unknown> | null = null;
  private channel: grpc.Channel | null = null;
  private state: GrpcClientState = "disconnected";
  private healthCheck: GrpcHealthCheck | null = null;

  constructor(options: GrpcClientOptions) {
    super();
    this.options = options;
  }

  /**
   * Connect to the gRPC service
   */
  async connect(): Promise<void> {
    if (this.state === "connected") {
      return;
    }

    if (this.state === "connecting") {
      return new Promise((resolve, reject) => {
        this.once("connected", resolve);
        this.once("error", reject);
      });
    }

    this.state = "connecting";
    this.emit("connecting");

    try {
      // Load proto file
      const packageDefinition = protoLoader.loadSync(this.options.protoPath, {
        keepCase: true,
        longs: String,
        enums: String,
        defaults: true,
        oneofs: true,
        includeDirs: this.options.protoIncludeDirs || [],
      });

      const packageObject = grpc.loadPackageDefinition(packageDefinition);

      // Navigate to the service (handle package.Service format)
      let serviceClass: Record<string, unknown> = packageObject;
      const serviceParts = this.options.serviceName.split(".");
      for (const part of serviceParts) {
        serviceClass = serviceClass[part] as Record<string, unknown>;
        if (!serviceClass) {
          throw new Error(`Service ${this.options.serviceName} not found in proto file`);
        }
      }

      // Create credentials
      let credentials: grpc.ChannelCredentials;
      if (this.options.useTls) {
        credentials = grpc.credentials.createSsl(this.options.rootCert);
      } else {
        credentials = grpc.credentials.createInsecure();
      }

      // Create channel and client
      this.channel = new grpc.Channel(this.options.address, credentials, {});
      this.client = new (serviceClass as unknown as new(addr: string, creds: grpc.ChannelCredentials) => Record<string, unknown>)(this.options.address, credentials);

      // Setup health check if enabled
      if (this.options.enableHealthCheck) {
        this.healthCheck = new GrpcHealthCheck({
          checkInterval: this.options.healthCheckInterval || 5000,
          onUnhealthy: () => this.handleUnhealthy(),
          onHealthy: () => this.handleHealthy(),
        });
        this.healthCheck.register(this.options.serviceName, () => this.performHealthCheck());
        this.healthCheck.start();
      }

      this.state = "connected";
      this.emit("connected");
    } catch (error) {
      this.state = "error";
      this.emit("error", error);
      throw error;
    }
  }

  /**
   * Close connection
   */
  async close(): Promise<void> {
    if (this.state === "disconnected" || this.state === "closed") {
      return;
    }

    this.state = "closing";

    try {
      if (this.healthCheck) {
        this.healthCheck.stop();
        this.healthCheck = null;
      }

      if (this.client) {
        // gRPC clients are closed via the channel
        if (this.channel) {
          this.channel.close();
        }
      }

      this.client = null;
      this.channel = null;
      this.state = "closed";
      this.emit("disconnected");
    } catch (error) {
      this.state = "error";
      this.emit("error", error);
      throw error;
    }
  }

  /**
   * Check if connected
   */
  isConnected(): boolean {
    return this.state === "connected";
  }

  /**
   * Get current state
   */
  getState(): GrpcClientState {
    return this.state;
  }

  /**
   * Generic RPC call
   */
  async call<TReq, TResp>(
    method: string,
    request: TReq,
    options?: { metadata?: Record<string, string>; timeout?: number },
  ): Promise<TResp> {
    if (!this.isConnected()) {
      throw new Error("gRPC client not connected");
    }

    if (!this.client || !this.client[method]) {
      throw new Error(`Method ${method} not found in service ${this.options.serviceName}`);
    }

    return new Promise((resolve, reject) => {
      const timeout = options?.timeout || this.options.defaultTimeout || 30000;
      const meta = new grpc.Metadata();

      if (options?.metadata) {
        for (const [key, value] of Object.entries(options.metadata)) {
          meta.set(key, value);
        }
      }

      // Set timeout
      const timeoutHandle = setTimeout(() => {
        reject(new Error(`RPC call timeout (${timeout}ms) for method ${method}`));
      }, timeout);

      const methodFn = (this.client as Record<string, unknown>)[method] as (req: TReq, meta: grpc.Metadata, cb: (err: grpc.ServiceError | null, resp: TResp) => void) => void;
      methodFn(request, meta, (error: grpc.ServiceError | null, response: TResp) => {
        clearTimeout(timeoutHandle);

        if (error) {
          reject(this.convertGrpcError(error));
        } else {
          resolve(response);
        }
      });
    });
  }

  /**
   * Convert gRPC error to standard format
   */
  private convertGrpcError(error: grpc.ServiceError): Error {
    const message = `gRPC error [${error.code}]: ${error.message}`;
    const err = new Error(message);
    ((err as unknown) as Record<string, unknown>)['code'] = error.code;
    ((err as unknown) as Record<string, unknown>)['details'] = error.details;
    return err;
  }

  /**
   * Perform health check
   */
  private async performHealthCheck(): Promise<boolean> {
    if (!this.isConnected()) {
      return false;
    }

    try {
      // Try calling a simple method (Status is common)
      if (this.client?.['Status']) {
        await new Promise<void>((resolve, reject) => {
          const timeout = 5000;
          const timeoutHandle = setTimeout(() => {
            reject(new Error("Health check timeout"));
          }, timeout);

          const statusFn = (this.client as Record<string, unknown>)['Status'] as (req: Record<string, unknown>, cb: (err: grpc.ServiceError | null) => void) => void;
          statusFn({}, (error: grpc.ServiceError | null) => {
            clearTimeout(timeoutHandle);
            if (error) {
              reject(error);
            } else {
              resolve();
            }
          });
        });
        return true;
      }

      // If no Status method, assume healthy
      return true;
    } catch {
      return false;
    }
  }

  /**
   * Handle unhealthy state
   */
  private handleUnhealthy(): void {
    this.emit("unhealthy");
  }

  /**
   * Handle healthy state
   */
  private handleHealthy(): void {
    this.emit("healthy");
  }
}
