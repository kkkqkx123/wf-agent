/**
 * Risk Level Tools
 * Provides purely instrumental functions related to risk levels
 *
 * Note: The SDK fully trusts the user-configured risk level and does not perform any pre-defined validation or assessment.
 * The application layer should implement customized risk assessment logic according to actual requirements.
 */

import { ScriptRiskLevel } from "@wf-agent/types";

/**
 * Get the priority value of the risk level
 * Used to compare risk levels
 *
 * @param riskLevel riskLevel
 * @returns Priority value (the higher the risk, the higher the risk)
 */
export function getRiskLevelPriority(riskLevel: ScriptRiskLevel): number {
  const priorityMap: Record<ScriptRiskLevel, number> = {
    none: 0,
    low: 1,
    medium: 2,
    high: 3,
  };
  return priorityMap[riskLevel];
}

/**
 * Comparing two risk levels
 *
 * @param riskLevel1 first risk level
 * @param riskLevel2 second risk level
 * @returns 1 means the first is higher, -1 means the second is higher, 0 means equal
 */
export function compareRiskLevels(
  riskLevel1: ScriptRiskLevel,
  riskLevel2: ScriptRiskLevel,
): number {
  const priority1 = getRiskLevelPriority(riskLevel1);
  const priority2 = getRiskLevelPriority(riskLevel2);

  if (priority1 > priority2) return 1;
  if (priority1 < priority2) return -1;
  return 0;
}
