/**
 * BaseBuilder - An abstract base class for builders
 * Provides common functionality for all builders: timestamp management, metadata management, and description settings
 */

import type { Metadata } from "@wf-agent/types";
import { now } from "@wf-agent/common-utils";

/**
 * BaseBuilder - Abstract Base Class for Builders
 */
export abstract class BaseBuilder<T> {
  protected _description?: string;
  protected _metadata: Metadata = {};
  protected _createdAt: number = now();
  protected _updatedAt: number = now();

  /**
   * Set the description
   * @param description The description
   * @returns this
   */
  description(description: string): this {
    this._description = description;
    this._updatedAt = now();
    return this;
  }

  /**
   * Set Metadata
   * @param metadata Metadata
   * @returns this
   */
  metadata(metadata: Metadata): this {
    this._metadata = metadata;
    this._updatedAt = now();
    return this;
  }

  /**
   * Set category
   * @param category The category
   * @returns this
   */
  category(category: string): this {
    if (!this._metadata) {
      this._metadata = {};
    }
    this._metadata["category"] = category;
    this._updatedAt = now();
    return this;
  }

  /**
   * Add Tags
   * @param tags An array of tags
   * @returns This
   */
  tags(...tags: string[]): this {
    if (!this._metadata) {
      this._metadata = {};
    }
    if (!this._metadata["tags"]) {
      this._metadata["tags"] = [];
    }
    (this._metadata["tags"] as string[]).push(...tags);
    this._updatedAt = now();
    return this;
  }

  /**
   * Add or update a metadata item
   * @param key: The metadata key
   * @param value: The metadata value
   * @returns: This
   */
  addMetadata(key: string, value: unknown): this {
    if (!this._metadata) {
      this._metadata = {};
    }
    this._metadata[key] = value;
    this._updatedAt = now();
    return this;
  }

  /**
   * Remove metadata items
   * @param key Metadata key
   * @returns this
   */
  removeMetadata(key: string): this {
    if (this._metadata) {
      delete this._metadata[key];
      this._updatedAt = now();
    }
    return this;
  }

  /**
   * Clear all metadata
   * @returns this
   */
  clearMetadata(): this {
    this._metadata = {};
    this._updatedAt = now();
    return this;
  }

  /**
   * Get the creation time
   * @returns Creation timestamp
   */
  getCreatedAt(): number {
    return this._createdAt;
  }

  /**
   * Get the update time
   * @returns Update timestamp
   */
  getUpdatedAt(): number {
    return this._updatedAt;
  }

  /**
   * Update timestamp
   */
  protected updateTimestamp(): void {
    this._updatedAt = now();
  }

  /**
   * Construct an object (abstract method; subclasses must implement it)
   * @returns The construction result
   */
  abstract build(): T;
}
