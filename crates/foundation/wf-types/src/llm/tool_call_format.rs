use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallFormat {
    Native,
    Xml,
    JsonWrapped,
    JsonRaw,
}

impl ToolCallFormat {
    /// All supported formats in canonical (snake_case) order.
    pub const ALL: [Self; 4] = [Self::Native, Self::Xml, Self::JsonWrapped, Self::JsonRaw];

    /// Whether two formats can interoperate at runtime (TS
    /// `validateToolFormatCompatibility`): identical formats always match,
    /// and the two JSON formats are interchangeable (markers may differ).
    pub fn is_compatible_with(&self, other: &Self) -> bool {
        self == other
            || matches!(
                (self, other),
                (Self::JsonWrapped, Self::JsonRaw) | (Self::JsonRaw, Self::JsonWrapped)
            )
    }
}

impl FromStr for ToolCallFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "native" => Ok(Self::Native),
            "xml" => Ok(Self::Xml),
            "json_wrapped" => Ok(Self::JsonWrapped),
            "json_raw" => Ok(Self::JsonRaw),
            other => Err(format!(
                "unsupported tool call format '{other}', expected one of {}",
                Self::ALL
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        }
    }
}

impl fmt::Display for ToolCallFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Native => "native",
            Self::Xml => "xml",
            Self::JsonWrapped => "json_wrapped",
            Self::JsonRaw => "json_raw",
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallMarkers {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
}

impl ToolCallMarkers {
    /// Default markers for wrapped JSON format: `<<<TOOL_CALL>>> ... <<<END_TOOL_CALL>>>`.
    pub fn default_json() -> Self {
        Self {
            start: Some("<<<TOOL_CALL>>>".to_string()),
            end: Some("<<<END_TOOL_CALL>>>".to_string()),
        }
    }
}

impl ToolCallFormatConfig {
    /// Resolve effective start marker, falling back to the JSON defaults.
    pub fn effective_start(&self) -> &str {
        self.markers
            .as_ref()
            .and_then(|m| m.start.as_deref())
            .unwrap_or("<<<TOOL_CALL>>>")
    }

    /// Resolve effective end marker.
    pub fn effective_end(&self) -> &str {
        self.markers
            .as_ref()
            .and_then(|m| m.end.as_deref())
            .unwrap_or("<<<END_TOOL_CALL>>>")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct XmlTags {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameter: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallFormatConfig {
    pub format: ToolCallFormat,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub markers: Option<ToolCallMarkers>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xml_tags: Option<XmlTags>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_description: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description_style: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_examples: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_rules: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_config: Option<HashMap<String, serde_json::Value>>,
}

impl ToolCallFormatConfig {
    /// Build a bare config from a raw format string (node or agent config
    /// `tool_call_format`), with no markers/tags overrides. Returns `None`
    /// for unknown format strings.
    pub fn from_format_str(s: &str) -> Option<Self> {
        let format = s.parse().ok()?;
        Some(Self {
            format,
            markers: None,
            xml_tags: None,
            include_description: None,
            description_style: None,
            include_examples: None,
            include_rules: None,
            additional_config: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_canonical_names() {
        assert_eq!("native".parse(), Ok(ToolCallFormat::Native));
        assert_eq!("xml".parse(), Ok(ToolCallFormat::Xml));
        assert_eq!("json_wrapped".parse(), Ok(ToolCallFormat::JsonWrapped));
        assert_eq!("json_raw".parse(), Ok(ToolCallFormat::JsonRaw));
        assert_eq!(
            " native ".parse::<ToolCallFormat>(),
            Ok(ToolCallFormat::Native)
        );
        assert!("yaml".parse::<ToolCallFormat>().is_err());
    }

    #[test]
    fn display_matches_serde_names() {
        for format in ToolCallFormat::ALL {
            let name = format.to_string();
            let roundtrip: ToolCallFormat =
                serde_json::from_value(serde_json::Value::String(name.clone())).unwrap();
            assert_eq!(roundtrip, format);
        }
    }

    #[test]
    fn bare_config_from_string() {
        let cfg = ToolCallFormatConfig::from_format_str("xml").unwrap();
        assert_eq!(cfg.format, ToolCallFormat::Xml);
        assert!(cfg.xml_tags.is_none());
        assert!(cfg.markers.is_none());
        assert!(ToolCallFormatConfig::from_format_str("yaml").is_none());
    }
}
