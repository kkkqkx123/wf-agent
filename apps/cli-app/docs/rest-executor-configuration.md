# REST Executor Configuration Guide

This document explains how to configure the REST executor at different levels.

## Configuration Hierarchy

REST tools support three levels of configuration:

1. **Executor-Level** (Global defaults for all REST tools)
2. **Tool-Level** (Per-tool overrides in TOML files)
3. **Runtime-Level** (Dynamic parameters during execution)

---

## 1. Executor-Level Configuration

Configure global defaults when creating the SDK instance:

```typescript
import { createSDK } from "@wf-agent/sdk";

const sdk = createSDK({
  // REST Executor Configuration
  restExecutorConfig: {
    // Global base URL (optional, tools can override)
    baseUrl: "https://api.example.com",
    
    // Default timeout for all REST requests (milliseconds)
    timeout: 30000,
    
    // Default headers for all requests
    headers: {
      "User-Agent": "MyAgent/1.0",
    },
    
    // Enable circuit breaker for fault tolerance
    enableCircuitBreaker: true,
    circuitBreaker: {
      failureThreshold: 5,      // Open circuit after 5 failures
      resetTimeout: 60000,      // Try half-open after 60 seconds
      halfOpenRequests: 3,      // Allow 3 test requests in half-open state
    },
    
    // Request interceptors (e.g., add auth tokens, logging)
    requestInterceptors: [
      {
        intercept: async (config) => {
          console.log(`Request to: ${config.url}`);
          return config;
        },
      },
    ],
    
    // Response interceptors (e.g., transform data, cache)
    responseInterceptors: [
      {
        intercept: async (response) => {
          console.log(`Response status: ${response.status}`);
          return response;
        },
      },
    ],
    
    // Error interceptors (e.g., custom error handling)
    errorInterceptors: [
      {
        intercept: async (error) => {
          console.error(`Request failed: ${error.message}`);
          return error;
        },
      },
    ],
  },
  
  // ... other SDK options
});

await sdk.waitForReady();
```

### When to Use Executor-Level Config

- Set default values that apply to **all** REST tools
- Configure infrastructure concerns (circuit breakers, rate limiters)
- Add cross-cutting concerns (logging, authentication)
- Define organization-wide policies

---

## 2. Tool-Level Configuration

Override global defaults in individual tool TOML files:

```toml
# configs/tools/rest/my-api.toml

name = "my_api"
tool_type = "rest"
description = "Custom API tool with specific configuration"

# Tool-specific REST configuration
[config]
# Override global baseUrl
baseUrl = "https://custom-api.example.com/v2"

# Override global timeout
timeout = 60000

# Tool-specific headers
[config.headers]
Authorization = "Bearer ${CUSTOM_API_KEY}"
X-API-Version = "2.0"

# Retry settings
maxRetries = 5
retryDelay = 2000

# Parameter schema
[parameters_schema]
type = "object"

[parameters_schema.properties]
endpoint = { type = "string", description = "API endpoint" }

parameters_schema.required = ["endpoint"]

[metadata]
category = "api"
tags = ["custom", "api"]
```

### Configuration Priority

When a REST tool executes, configuration is merged in this order:

1. **Executor-Level** (lowest priority - global defaults)
2. **Tool-Level** (medium priority - TOML config)
3. **Runtime Parameters** (highest priority - LLM-provided)

Example:
```typescript
// Executor config: timeout = 30000
// Tool config: timeout = 60000
// Runtime: no timeout specified
// Result: Uses 60000 (from tool config)

// Runtime: timeout = 90000
// Result: Uses 90000 (from runtime)
```

---

## 3. Runtime-Level Configuration

Pass dynamic parameters during tool execution:

```typescript
const result = await sdk.tools.execute("fetch", {
  // These override both executor and tool configs
  url: "https://example.com/api/data",
  method: "POST",
  headers: {
    "Content-Type": "application/json",
    "Authorization": "Bearer dynamic-token",
  },
  body: {
    query: "search term",
  },
});
```

### Available Runtime Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `url` or `endpoint` | string | Request URL (required) |
| `method` | string | HTTP method (GET, POST, PUT, DELETE, PATCH) |
| `body` | object | Request body for POST/PUT/PATCH |
| `headers` | object | Request headers (merged with config headers) |
| `query` or `params` | object | Query parameters (appended to URL) |

---

## Complete Example

### Step 1: Configure SDK

```typescript
const sdk = createSDK({
  restExecutorConfig: {
    timeout: 30000,
    enableCircuitBreaker: true,
  },
});
```

### Step 2: Create Tool Config

```toml
# configs/tools/rest/weather.toml
name = "weather"
tool_type = "rest"
description = "Get weather information"

[config]
baseUrl = "https://api.weather.com/v1"
timeout = 15000
maxRetries = 3

[config.headers]
X-API-Key = "${WEATHER_API_KEY}"

[parameters_schema]
type = "object"

[parameters_schema.properties]
city = { type = "string", description = "City name" }
country = { type = "string", description = "Country code", default = "US" }

parameters_schema.required = ["city"]

[metadata]
category = "weather"
tags = ["weather", "api"]
```

### Step 3: Register Tool

```typescript
// Load from file
const { content } = await loadConfigContent("./configs/tools/rest/weather.toml");
const tool = parseToml(content) as Tool;

await sdk.tools.create(tool);
```

### Step 4: Execute Tool

```typescript
const result = await sdk.tools.execute("weather", {
  city: "Beijing",
  country: "CN",
});

console.log(result);
// Uses baseUrl from tool config + runtime params
// Final URL: https://api.weather.com/v1?city=Beijing&country=CN
```

---

## Best Practices

### 1. Use Executor-Level for Infrastructure

```typescript
// ✅ Good: Global infrastructure config
restExecutorConfig: {
  enableCircuitBreaker: true,
  requestInterceptors: [authInterceptor],
}

// ❌ Bad: Don't put tool-specific URLs here
restExecutorConfig: {
  baseUrl: "https://specific-api.com", // Too specific!
}
```

### 2. Use Tool-Level for Service-Specific Settings

```toml
# ✅ Good: Each service has its own config
[config]
baseUrl = "https://weather-api.com"
timeout = 15000

# ❌ Bad: Don't repeat global settings
[config]
enableCircuitBreaker = true  # Already set at executor level
```

### 3. Use Runtime for Dynamic Data

```typescript
// ✅ Good: Dynamic parameters at runtime
await sdk.tools.execute("api", {
  url: userInput,  // User-provided URL
});

// ❌ Bad: Hardcoding in tool config
[config]
baseUrl = "https://hardcoded-url.com"  # Inflexible!
```

---

## Troubleshooting

### Issue: Configuration Not Applied

**Check:**
1. Is the tool type `"rest"`?
2. Does the TOML file have `[config]` section?
3. Are you passing `restExecutorConfig` to `createSDK()`?

**Debug:**
```typescript
const tool = sdk.tools.get("my_tool");
console.log("Tool config:", tool.config);
```

### Issue: Headers Not Sent

**Check:**
1. Headers are defined in correct location (`[config.headers]`)
2. No typos in header names
3. Runtime headers don't override config headers unintentionally

**Debug:**
```typescript
// Add request interceptor to log headers
requestInterceptors: [{
  intercept: (config) => {
    console.log("Request headers:", config.headers);
    return config;
  }
}]
```

### Issue: Timeout Not Working

**Check:**
1. Timeout is in milliseconds (not seconds)
2. Value is positive number
3. Not overridden by runtime parameters

**Example:**
```toml
[config]
timeout = 30000  # 30 seconds ✅
timeout = 30     # 30 milliseconds ❌
```

---

## Migration Guide

### From Old Format (No Config Support)

**Before:**
```toml
name = "api"
tool_type = "rest"
# No config section
```

**After:**
```toml
name = "api"
tool_type = "rest"

[config]
baseUrl = "https://api.example.com"
timeout = 30000
```

### Steps:
1. Add `[config]` section to TOML files
2. Move relevant settings from runtime to config
3. Update SDK initialization with `restExecutorConfig`
4. Test tools to ensure configuration is applied

---

## Advanced Topics

### Custom Interceptors

```typescript
// Logging interceptor
const loggingInterceptor = {
  intercept: async (config) => {
    const startTime = Date.now();
    console.log(`[${config.method}] ${config.url}`);
    
    // Add timestamp to headers
    config.headers["X-Request-Timestamp"] = String(startTime);
    
    return config;
  },
};

// Response timing interceptor
const timingInterceptor = {
  intercept: async (response) => {
    const timestamp = response.headers?.["X-Request-Timestamp"];
    if (timestamp) {
      const duration = Date.now() - Number(timestamp);
      console.log(`Response time: ${duration}ms`);
    }
    return response;
  },
};
```

### Environment Variable Substitution

TOML files support environment variable placeholders:

```toml
[config.headers]
Authorization = "Bearer ${API_KEY}"
X-Service-Url = "${SERVICE_URL}"
```

Make sure to set environment variables before starting the application:

```bash
export API_KEY="your-secret-key"
export SERVICE_URL="https://api.example.com"
```

### Circuit Breaker Patterns

```typescript
restExecutorConfig: {
  enableCircuitBreaker: true,
  circuitBreaker: {
    // After 5 consecutive failures, open the circuit
    failureThreshold: 5,
    
    // Wait 60 seconds before trying again (half-open state)
    resetTimeout: 60000,
    
    // Allow 3 test requests in half-open state
    halfOpenRequests: 3,
  },
}
```

**States:**
- **Closed**: Normal operation
- **Open**: Failing fast, rejecting requests
- **Half-Open**: Testing if service recovered

---

## Summary

| Level | Location | Purpose | Example |
|-------|----------|---------|---------|
| Executor | SDK init | Global defaults | `createSDK({ restExecutorConfig })` |
| Tool | TOML file | Per-tool overrides | `[config]` section |
| Runtime | Execution call | Dynamic params | `sdk.tools.execute(id, params)` |

**Remember:**
- Executor-level for infrastructure
- Tool-level for service configuration
- Runtime-level for dynamic data
- Higher levels override lower levels
