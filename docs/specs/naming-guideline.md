## Module Design & Naming Guidelines

### Core Architecture Principles (The "Why")

**Principle 1: No "Version 2" of a Module**
There is only the module itself, evolving in place. Never create a new module that implies it replaces or improves an existing one.

**Principle 2: Single Responsibility per Module**
A module does one thing. If you find yourself mixing data structure + operations + config + caching + monitoring — split along responsibility lines, not "optimization" lines.

**Principle 3: Optimization is Modification, Not Replacement**
Performance improvements, simplifications, and cleanups happen **inside** the original module. Creating a new module for these purposes is forbidden.

**Principle 4: Neutral Naming**
Names describe **what the module does**, not **how it compares to another version** (better, simpler, smarter, cleaner).

---

### When to Split a Module (And When Not To)

| Situation | Action |
|-----------|--------|
| Module has 2+ distinct responsibilities | Split into **responsibility-named** modules (e.g., `Parser` + `Validator`) |
| Module works fine but could be "better" | Optimize in place |
| You want a simplified version for some callers | Edit the original |
| Dead code / cleanup needed | Delete in place. Never create `CleanXxx` |
| Original is too complex to understand | Refactor the original. Split if responsibilities are entangled — but name neutrally |

---

### Naming Guidelines (The "How")

**Names should answer: "What does this module do?"** not "How is this module different from another?"

| Recommended | Avoid (Because it compares) |
|-------------|----------------------------|
| `BatchProcessor` | `OptimizedBatchProcessor` |
| `CacheManager` | `LightweightCacheManager` |
| `DataValidator` | `SimplifiedDataValidator` |
| `IndexCoordinator` | `UnifiedIndexCoordinator` |
| `MemoryBuffer` | `SmartMemoryBuffer` |
| `RetryHandler` | `AdvancedRetryHandler` |

**Context matters:** Some terms are legitimate domain vocabulary, not comparisons, like:
- ✅ `SmartPointer` (C++ standard term) — acceptable
- ✅ `CleanArchitecture` (established pattern name) — acceptable if referring to the pattern, not claiming "cleaner than ours"
- ❌ `CleanConfigParser` (implies original was dirty)

**Rule of thumb:** If the term compares this module to a hypothetical other version of itself → forbidden. If it's an established industry term with no local "original" → allowed.