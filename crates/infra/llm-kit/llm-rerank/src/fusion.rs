//! Score-fusion strategy combining rerank scores with initial scores.
//!
//! Ported from code-context-engine `cce-config` search module: plain-string
//! TOML forms (`"rerank_only"`, `"linear_weighted"`, `"multiplicative"`,
//! `"rrf"` / `"reciprocal_rank_fusion"`) resolve to defaults, while
//! single-key maps (`{ linear_weighted = { alpha = 0.8 } }`) tune parameters.

use serde::{Deserialize, Serialize};

/// Strategy fusing the rerank score with the pre-rerank initial score.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RerankFusionStrategy {
    /// Uses only the rerank score.
    RerankOnly,
    /// Weighted blend: `alpha * rerank + (1 - alpha) * initial`.
    LinearWeighted {
        /// Weight of the rerank score in `[0, 1]`.
        alpha: f32,
    },
    /// Product fusion: `rerank * initial`.
    Multiplicative,
    /// Rank fusion over both orders:
    /// `1/(k + rerank_rank + 1) + 1/(k + initial_rank + 1)` with zero-based
    /// ranks. Ignores raw magnitudes, staying robust across incompatible
    /// score scales.
    #[serde(alias = "rrf")]
    ReciprocalRankFusion {
        /// Rank smoothing constant.
        k: f32,
    },
}

impl Default for RerankFusionStrategy {
    fn default() -> Self {
        Self::LinearWeighted {
            alpha: default_alpha(),
        }
    }
}

fn default_alpha() -> f32 {
    0.7
}

fn default_rrf_k() -> f32 {
    60.0
}

#[derive(Deserialize)]
struct LinearWeightedParams {
    #[serde(default = "default_alpha")]
    alpha: f32,
}

#[derive(Deserialize)]
struct RankFusionParams {
    #[serde(default = "default_rrf_k")]
    k: f32,
}

impl<'de> Deserialize<'de> for RerankFusionStrategy {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, MapAccess, Visitor};

        struct StrategyVisitor;

        impl<'de> Visitor<'de> for StrategyVisitor {
            type Value = RerankFusionStrategy;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str(
                    "a fusion strategy string (\"rerank_only\", \"linear_weighted\", \
                     \"multiplicative\", \"reciprocal_rank_fusion\"/\"rrf\") or a single-key map",
                )
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                match value {
                    "rerank_only" => Ok(RerankFusionStrategy::RerankOnly),
                    "linear_weighted" => Ok(RerankFusionStrategy::LinearWeighted {
                        alpha: default_alpha(),
                    }),
                    "multiplicative" => Ok(RerankFusionStrategy::Multiplicative),
                    "reciprocal_rank_fusion" | "rrf" => {
                        Ok(RerankFusionStrategy::ReciprocalRankFusion { k: default_rrf_k() })
                    }
                    _ => Err(de::Error::unknown_variant(
                        value,
                        &[
                            "rerank_only",
                            "linear_weighted",
                            "multiplicative",
                            "reciprocal_rank_fusion",
                            "rrf",
                        ],
                    )),
                }
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let name: Option<String> = map.next_key()?;
                let Some(name) = name else {
                    return Err(de::Error::invalid_length(0, &"a single-key strategy map"));
                };
                let strategy = match name.as_str() {
                    "rerank_only" => {
                        let _: de::IgnoredAny = map.next_value()?;
                        RerankFusionStrategy::RerankOnly
                    }
                    "linear_weighted" => {
                        let params: LinearWeightedParams = map.next_value()?;
                        RerankFusionStrategy::LinearWeighted {
                            alpha: params.alpha,
                        }
                    }
                    "multiplicative" => {
                        let _: de::IgnoredAny = map.next_value()?;
                        RerankFusionStrategy::Multiplicative
                    }
                    "reciprocal_rank_fusion" | "rrf" => {
                        let params: RankFusionParams = map.next_value()?;
                        RerankFusionStrategy::ReciprocalRankFusion { k: params.k }
                    }
                    other => {
                        return Err(de::Error::unknown_variant(
                            other,
                            &[
                                "rerank_only",
                                "linear_weighted",
                                "multiplicative",
                                "reciprocal_rank_fusion",
                                "rrf",
                            ],
                        ));
                    }
                };
                if map.next_key::<de::IgnoredAny>()?.is_some() {
                    return Err(de::Error::invalid_length(2, &"a single-key strategy map"));
                }
                Ok(strategy)
            }
        }

        deserializer.deserialize_any(StrategyVisitor)
    }
}

impl RerankFusionStrategy {
    /// Calculates the final score from rerank and initial scores.
    ///
    /// Both ranks are zero-based positions: `rerank_rank` in the reranked
    /// order, `initial_rank` in the pre-rerank order. Only the rank-fusion
    /// variant reads them; score-based variants ignore both.
    pub fn calculate(
        &self,
        rerank_score: f32,
        initial_score: f32,
        rerank_rank: usize,
        initial_rank: usize,
    ) -> f32 {
        match self {
            Self::RerankOnly => rerank_score,
            Self::LinearWeighted { alpha } => alpha * rerank_score + (1.0 - alpha) * initial_score,
            Self::Multiplicative => rerank_score * initial_score,
            Self::ReciprocalRankFusion { k } => {
                let rerank_denominator = *k + rerank_rank as f32 + 1.0;
                let initial_denominator = *k + initial_rank as f32 + 1.0;
                if !rerank_denominator.is_finite()
                    || !initial_denominator.is_finite()
                    || rerank_denominator <= 0.0
                    || initial_denominator <= 0.0
                {
                    rerank_score
                } else {
                    1.0 / rerank_denominator + 1.0 / initial_denominator
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_variant_fuses_scores() {
        assert!(
            (RerankFusionStrategy::RerankOnly.calculate(0.9, 0.8, 0, 1) - 0.9).abs() < f32::EPSILON
        );
        let blended = RerankFusionStrategy::LinearWeighted { alpha: 0.7 }.calculate(0.9, 0.8, 0, 1);
        assert!((blended - (0.7 * 0.9 + 0.3 * 0.8)).abs() < f32::EPSILON);
        assert!(
            (RerankFusionStrategy::Multiplicative.calculate(0.9, 0.8, 0, 1) - 0.72).abs() < 1e-6
        );
        let rrf = RerankFusionStrategy::ReciprocalRankFusion { k: 60.0 }.calculate(0.9, 0.8, 0, 0);
        assert!((rrf - (1.0 / 61.0 + 1.0 / 61.0)).abs() < 1e-6);
    }

    #[test]
    fn string_forms_resolve_to_defaults() {
        assert_eq!(
            serde_json::from_str::<RerankFusionStrategy>(r#""rerank_only""#).expect("parsed"),
            RerankFusionStrategy::RerankOnly
        );
        assert_eq!(
            serde_json::from_str::<RerankFusionStrategy>(r#""linear_weighted""#).expect("parsed"),
            RerankFusionStrategy::LinearWeighted { alpha: 0.7 }
        );
        assert_eq!(
            serde_json::from_str::<RerankFusionStrategy>(r#""rrf""#).expect("parsed"),
            RerankFusionStrategy::ReciprocalRankFusion { k: 60.0 }
        );
    }

    #[test]
    fn single_key_maps_tune_parameters() {
        let strategy =
            serde_json::from_str::<RerankFusionStrategy>(r#"{"linear_weighted":{"alpha":0.8}}"#)
                .expect("parsed");
        assert_eq!(
            strategy,
            RerankFusionStrategy::LinearWeighted { alpha: 0.8 }
        );

        let strategy =
            serde_json::from_str::<RerankFusionStrategy>(r#"{"rrf":{"k":30.0}}"#).expect("parsed");
        assert_eq!(
            strategy,
            RerankFusionStrategy::ReciprocalRankFusion { k: 30.0 }
        );
    }

    #[test]
    fn unknown_strategies_are_rejected() {
        assert!(serde_json::from_str::<RerankFusionStrategy>(r#""unknown""#).is_err());
    }
}
