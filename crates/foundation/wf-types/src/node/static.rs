use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

/// Node types of the static workflow graph. The closed variants cover the
/// builtin engine handlers; [`StaticNodeType::Custom`] carries node types
/// contributed by plugins, which are only known at runtime and therefore
/// cannot be enum variants.
///
/// Serialization mirrors the graph JSON syntax: every variant round-trips as
/// a bare SCREAMING_SNAKE_CASE string (`"START"`, `"LLM"`, or the custom
/// name itself), so plugin-contributed types can appear in workflow graphs
/// without a wrapper object.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StaticNodeType {
    Start,
    End,
    EmbedStart,
    EmbedEnd,
    Variable,
    Fork,
    Join,
    Sync,
    Subgraph,
    EmbedGraph,
    Script,
    InteractiveScript,
    Llm,
    ToolVisibility,
    UserInteraction,
    Route,
    ContextProcessor,
    LoopStart,
    LoopEnd,
    AgentLoop,
    StartFromMessage,
    ContinueFromMessage,
    /// A plugin-contributed node type identified by its registered name.
    Custom(String),
}

impl StaticNodeType {
    /// All known node type variant names in SCREAMING_SNAKE_CASE.
    pub const ALL: &'static [&'static str] = &[
        "START",
        "END",
        "VARIABLE",
        "LLM",
        "SCRIPT",
        "TOOL_VISIBILITY",
        "FORK",
        "JOIN",
        "SYNC",
        "LOOP_START",
        "LOOP_END",
        "ROUTE",
        "SUBGRAPH",
        "EMBED_GRAPH",
        "AGENT_LOOP",
        "INTERACTIVE_SCRIPT",
        "USER_INTERACTION",
        "START_FROM_MESSAGE",
        "CONTINUE_FROM_MESSAGE",
        "EMBED_START",
        "EMBED_END",
        "CONTEXT_PROCESSOR",
    ];

    /// Parse a node type string (case-insensitive). Returns `None` for
    /// unknown values.
    pub fn from_str_ci(value: &str) -> Option<Self> {
        match value.to_uppercase().as_str() {
            "START" => Some(Self::Start),
            "END" => Some(Self::End),
            "EMBED_START" => Some(Self::EmbedStart),
            "EMBED_END" => Some(Self::EmbedEnd),
            "VARIABLE" => Some(Self::Variable),
            "FORK" => Some(Self::Fork),
            "JOIN" => Some(Self::Join),
            "SYNC" => Some(Self::Sync),
            "SUBGRAPH" => Some(Self::Subgraph),
            "EMBED_GRAPH" => Some(Self::EmbedGraph),
            "SCRIPT" => Some(Self::Script),
            "INTERACTIVE_SCRIPT" => Some(Self::InteractiveScript),
            "LLM" => Some(Self::Llm),
            "TOOL_VISIBILITY" => Some(Self::ToolVisibility),
            "USER_INTERACTION" => Some(Self::UserInteraction),
            "ROUTE" => Some(Self::Route),
            "CONTEXT_PROCESSOR" => Some(Self::ContextProcessor),
            "LOOP_START" => Some(Self::LoopStart),
            "LOOP_END" => Some(Self::LoopEnd),
            "AGENT_LOOP" => Some(Self::AgentLoop),
            "START_FROM_MESSAGE" => Some(Self::StartFromMessage),
            "CONTINUE_FROM_MESSAGE" => Some(Self::ContinueFromMessage),
            _ => None,
        }
    }

    /// Node types that represent an entire nested execution (child graphs,
    /// agent loops, long interactive sessions). They carry their own
    /// wall-clock / round budgets internally, so the engine-wide fallback
    /// node timeout must not wrap them (it would kill the inner chain that
    /// has its own retry budget); an explicitly configured node or options
    /// timeout still applies.
    pub fn is_long_running(&self) -> bool {
        matches!(
            self,
            Self::AgentLoop
                | Self::Subgraph
                | Self::EmbedGraph
                | Self::UserInteraction
                | Self::InteractiveScript
        )
    }
}

impl StaticNodeType {
    fn as_static_str(&self) -> Option<&'static str> {
        match self {
            Self::Start => Some("START"),
            Self::End => Some("END"),
            Self::EmbedStart => Some("EMBED_START"),
            Self::EmbedEnd => Some("EMBED_END"),
            Self::Variable => Some("VARIABLE"),
            Self::Fork => Some("FORK"),
            Self::Join => Some("JOIN"),
            Self::Sync => Some("SYNC"),
            Self::Subgraph => Some("SUBGRAPH"),
            Self::EmbedGraph => Some("EMBED_GRAPH"),
            Self::Script => Some("SCRIPT"),
            Self::InteractiveScript => Some("INTERACTIVE_SCRIPT"),
            Self::Llm => Some("LLM"),
            Self::ToolVisibility => Some("TOOL_VISIBILITY"),
            Self::UserInteraction => Some("USER_INTERACTION"),
            Self::Route => Some("ROUTE"),
            Self::ContextProcessor => Some("CONTEXT_PROCESSOR"),
            Self::LoopStart => Some("LOOP_START"),
            Self::LoopEnd => Some("LOOP_END"),
            Self::AgentLoop => Some("AGENT_LOOP"),
            Self::StartFromMessage => Some("START_FROM_MESSAGE"),
            Self::ContinueFromMessage => Some("CONTINUE_FROM_MESSAGE"),
            Self::Custom(_) => None,
        }
    }
}

impl Serialize for StaticNodeType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.as_static_str() {
            Some(name) => serializer.serialize_str(name),
            None => match self {
                Self::Custom(name) => serializer.serialize_str(name),
                _ => unreachable!("non-custom variants always have a static name"),
            },
        }
    }
}

impl<'de> Deserialize<'de> for StaticNodeType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::from_str_ci(&value).ok_or_else(|| {
            de::Error::invalid_value(
                de::Unexpected::Str(&value),
                &"a known node type or a plugin-contributed type name",
            )
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BaseStaticNode {
    pub id: super::super::Id,
    pub node_type: StaticNodeType,
    pub name: Option<String>,
    pub description: Option<String>,
    pub config: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_config: Option<super::NodeExecutionConfig>,
}
