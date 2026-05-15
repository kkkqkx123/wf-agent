/**
 * Prometheus Formatter Utility
 * 
 * Provides utilities for formatting metrics in Prometheus exposition format.
 * Follows the official Prometheus text-based exposition format specification.
 */

/**
 * Prometheus metric type
 */
export type PrometheusMetricType = 
  | 'counter'
  | 'gauge'
  | 'histogram'
  | 'summary';

/**
 * Prometheus metric definition
 */
export interface PrometheusMetric {
  /** Metric name (snake_case) */
  name: string;
  /** Metric type */
  type: PrometheusMetricType;
  /** Help text */
  help: string;
  /** Labels and value */
  samples: PrometheusSample[];
}

/**
 * Prometheus sample (a single metric line)
 */
export interface PrometheusSample {
  /** Label key-value pairs */
  labels?: Record<string, string>;
  /** Metric value */
  value: number;
  /** Optional timestamp (Unix milliseconds) */
  timestamp?: number;
}

/**
 * Utility class for formatting metrics in Prometheus exposition format
 */
export class PrometheusFormatter {
  /**
   * Format a complete Prometheus metric with HELP, TYPE, and samples
   */
  static formatMetric(metric: PrometheusMetric): string[] {
    const lines: string[] = [];
    
    // Add HELP comment
    lines.push(`# HELP ${metric.name} ${metric.help}`);
    
    // Add TYPE declaration
    lines.push(`# TYPE ${metric.name} ${metric.type}`);
    
    // Add samples
    for (const sample of metric.samples) {
      lines.push(this.formatSample(metric.name, sample));
    }
    
    return lines;
  }
  
  /**
   * Format a single metric sample
   */
  static formatSample(name: string, sample: PrometheusSample): string {
    let line = name;
    
    // Add labels if present
    if (sample.labels && Object.keys(sample.labels).length > 0) {
      const labelStr = this.formatLabels(sample.labels);
      line += labelStr;
    }
    
    // Add value
    line += ` ${sample.value}`;
    
    // Add timestamp if present
    if (sample.timestamp) {
      line += ` ${sample.timestamp}`;
    }
    
    return line;
  }
  
  /**
   * Format labels in Prometheus format: {label1="value1",label2="value2"}
   */
  static formatLabels(labels: Record<string, string>): string {
    if (!labels || Object.keys(labels).length === 0) {
      return '';
    }
    
    const parts = Object.entries(labels)
      .map(([key, value]) => `${key}="${this.escapeLabelValue(value)}"`)
      .join(',');
    
    return `{${parts}}`;
  }
  
  /**
   * Escape special characters in label values
   */
  private static escapeLabelValue(value: string): string {
    return value
      .replace(/\\/g, '\\\\')
      .replace(/"/g, '\\"')
      .replace(/\n/g, '\\n');
  }
  
  /**
   * Combine multiple metrics into final output
   */
  static combine(metrics: string[][], addTimestamp: boolean = true): string {
    const allLines: string[] = [];
    
    for (const metricLines of metrics) {
      allLines.push(...metricLines);
    }
    
    // Add generation timestamp
    if (addTimestamp) {
      allLines.push(`# Generated at ${new Date().toISOString()}`);
    }
    
    return allLines.join('\n') + '\n';
  }
}
