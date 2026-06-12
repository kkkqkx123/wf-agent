import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { TokenMetricsCollector } from "../token-collector.js";

describe("TokenMetricsCollector", () => {
  let collector: TokenMetricsCollector;

  beforeEach(() => {
    collector = new TokenMetricsCollector();
  });

  afterEach(() => {
    collector.dispose();
  });

  describe("recordTokenUsage", () => {
    it("should record token usage metrics", () => {
      collector.recordTokenUsage({
        profileId: "profile-1",
        executionId: "exec-1",
        nodeId: "node-1",
        totalTokens: 1000,
        promptTokens: 600,
        completionTokens: 400,
        cost: 0.02,
      });
      const totalResult = collector.query({ metricName: "token.usage.total" });
      expect(totalResult.totalCount).toBe(1);
      const promptResult = collector.query({ metricName: "token.usage.prompt" });
      expect(promptResult.totalCount).toBe(1);
      const completionResult = collector.query({ metricName: "token.usage.completion" });
      expect(completionResult.totalCount).toBe(1);
      const costResult = collector.query({ metricName: "token.cost.total" });
      expect(costResult.totalCount).toBe(1);
      const requestResult = collector.query({ metricName: "token.request.count" });
      expect(requestResult.totalCount).toBe(1);
    });

    it("should record without optional fields", () => {
      collector.recordTokenUsage({
        profileId: "profile-1",
        totalTokens: 500,
        promptTokens: 300,
        completionTokens: 200,
      });
      const totalResult = collector.query({ metricName: "token.usage.total" });
      expect(totalResult.totalCount).toBe(1);
    });

    it("should not record cost when undefined or zero", () => {
      collector.recordTokenUsage({
        profileId: "profile-1",
        totalTokens: 100,
        promptTokens: 50,
        completionTokens: 50,
      });
      const costResult = collector.query({ metricName: "token.cost.total" });
      expect(costResult.totalCount).toBe(0);
    });
  });

  describe("recordTokenWarning", () => {
    it("should record token warning gauge", () => {
      collector.recordTokenWarning("profile-1", 800, 1000, 80);
      const result = collector.query({ metricName: "token.usage.warning.percentage" });
      expect(result.totalCount).toBe(1);
    });
  });

  describe("getTokenStatsByProfile", () => {
    it("should return all token stats when no profile filter", () => {
      collector.recordTokenUsage({
        profileId: "profile-1",
        totalTokens: 100,
        promptTokens: 50,
        completionTokens: 50,
      });
      const result = collector.getTokenStatsByProfile();
      expect(result.totalCount).toBeGreaterThan(0);
    });

    it("should filter by profileId", () => {
      collector.recordTokenUsage({
        profileId: "profile-1",
        totalTokens: 100,
        promptTokens: 50,
        completionTokens: 50,
      });
      collector.recordTokenUsage({
        profileId: "profile-2",
        totalTokens: 200,
        promptTokens: 100,
        completionTokens: 100,
      });
      const result = collector.getTokenStatsByProfile("profile-1");
      expect(result.totalCount).toBeGreaterThan(0);
    });
  });

  describe("getTokenUsageSummary", () => {
    it("should return summary with zeros when no data", () => {
      const summary = collector.getTokenUsageSummary();
      expect(summary.totalTokens).toBe(0);
      expect(summary.totalPromptTokens).toBe(0);
      expect(summary.totalCompletionTokens).toBe(0);
      expect(summary.totalCost).toBe(0);
      expect(summary.totalRequests).toBe(0);
      expect(summary.byProfile.size).toBe(0);
    });

    it("should aggregate token usage values", () => {
      collector.recordTokenUsage({
        profileId: "profile-1",
        totalTokens: 1000,
        promptTokens: 600,
        completionTokens: 400,
        cost: 0.02,
      });
      collector.recordTokenUsage({
        profileId: "profile-1",
        totalTokens: 500,
        promptTokens: 300,
        completionTokens: 200,
        cost: 0.01,
      });
      const summary = collector.getTokenUsageSummary();
      expect(summary.totalTokens).toBe(1500);
      expect(summary.totalRequests).toBe(2);
      expect(summary.byProfile.has("profile-1")).toBe(true);
    });
  });

  describe("getAverageTokensPerRequest", () => {
    it("should return empty map when no data", () => {
      const averages = collector.getAverageTokensPerRequest();
      expect(averages.size).toBe(0);
    });

    it("should calculate averages after token usage", () => {
      collector.recordTokenUsage({
        profileId: "profile-1",
        totalTokens: 1000,
        promptTokens: 600,
        completionTokens: 400,
      });
      collector.recordTokenUsage({
        profileId: "profile-1",
        totalTokens: 500,
        promptTokens: 300,
        completionTokens: 200,
      });
      const averages = collector.getAverageTokensPerRequest();
      expect(averages.has("profile-1")).toBe(true);
    });
  });

  describe("toPrometheus", () => {
    it("should export in Prometheus format", () => {
      collector.recordTokenUsage({
        profileId: "profile-1",
        totalTokens: 100,
        promptTokens: 50,
        completionTokens: 50,
      });
      const lines = collector.toPrometheus();
      expect(lines.length).toBeGreaterThan(0);
      expect(lines.some(l => l.includes("token_usage_total"))).toBe(true);
    });
  });

  describe("toJSON", () => {
    it("should export as JSON", () => {
      const json = collector.toJSON();
      expect(json).toHaveProperty("type", "token");
    });
  });
});
