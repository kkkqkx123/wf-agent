/**
 * TemplateBuilder - Abstract base class for template builders.
 * Inherits from BaseBuilder and adds template registration functionality.
 */

import { BaseBuilder } from "../../shared/base-builder.js";

/**
 * TemplateBuilder - Abstract base class for template builders.
 */
export abstract class TemplateBuilder<T> extends BaseBuilder<T> {
  /**
   * Registering templates to the global registry
   * @returns this
   */
  register(): this {
    const template = this.build();
    this.registerTemplate(template);
    return this;
  }

  /**
   * Building and Registering Templates
   * @returns the template object
   */
  buildAndRegister(): T {
    const template = this.build();
    this.registerTemplate(template);
    return template;
  }

  /**
   * Register template to registry (abstract method, subclass must implement)
   * @param template template object
   */
  protected abstract registerTemplate(template: T): void;
}
