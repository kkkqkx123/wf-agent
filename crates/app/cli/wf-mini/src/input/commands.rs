//! Slash commands accepted at the prompt.

/// Slash commands accepted at the prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashCommand {
    Quit,
    Help,
    Clear,
    New,
    Checkpoints,
    History(Option<String>),
    Rerun(Option<usize>),
    Markdown(Option<String>),
    Reasoning(Option<String>),
    Usage(Option<String>),
    Tools(Option<String>),
    Display,
    Recap,
    Steer {
        node: Option<String>,
        text: String,
    },
    Followup {
        node: Option<String>,
        text: String,
    },
    AppendMode {
        node: Option<String>,
        mode: Option<String>,
    },
    ConvertPending {
        node: Option<String>,
    },
    Loops(Option<String>),
}

/// Split an optional leading `--node <id>` (or `--node=<id>`) flag from
/// the remaining arguments. The flag selects a nested workflow node
/// explicitly; its absence means the session default applies.
fn split_node_flag(args: &[&str]) -> Option<(Option<String>, Vec<String>)> {
    let (node, rest) = match args.first() {
        Some(first) if *first == "--node" => {
            let id = args.get(1)?;
            (Some((*id).to_string()), args.get(2..).unwrap_or(&[]))
        }
        Some(first) => match first.strip_prefix("--node=") {
            Some(id) if !id.is_empty() => (Some(id.to_string()), args.get(1..).unwrap_or(&[])),
            _ => (None, args),
        },
        None => (None, args),
    };
    Some((node, rest.iter().map(|item| (*item).to_string()).collect()))
}

/// Parse a trimmed input line into a slash command, if it is one.
///
/// `/rerun` takes an optional 1-based history number (`/rerun 3`); a
/// malformed variant is plain text, like any other unknown command.
pub fn parse_command(line: &str) -> Option<SlashCommand> {
    let mut tokens = line.split_whitespace();
    match tokens.next()? {
        "/quit" | "/exit" | ":q" if tokens.next().is_none() => Some(SlashCommand::Quit),
        "/help" | "/?" if tokens.next().is_none() => Some(SlashCommand::Help),
        "/clear" if tokens.next().is_none() => Some(SlashCommand::Clear),
        "/new" if tokens.next().is_none() => Some(SlashCommand::New),
        "/checkpoints" if tokens.next().is_none() => Some(SlashCommand::Checkpoints),
        "/history" => match tokens.next() {
            None => Some(SlashCommand::History(None)),
            Some(prefix) if tokens.next().is_none() => {
                Some(SlashCommand::History(Some(prefix.to_string())))
            }
            _ => None,
        },
        "/rerun" => match tokens.next() {
            None => Some(SlashCommand::Rerun(None)),
            Some(number) => match number.parse::<usize>() {
                Ok(n) if n >= 1 && tokens.next().is_none() => Some(SlashCommand::Rerun(Some(n))),
                _ => None,
            },
        },
        "/markdown" => match tokens.next() {
            None => Some(SlashCommand::Markdown(None)),
            Some(mode) if tokens.next().is_none() => {
                Some(SlashCommand::Markdown(Some(mode.to_string())))
            }
            _ => None,
        },
        "/reasoning" => match tokens.next() {
            None => Some(SlashCommand::Reasoning(None)),
            Some(toggle) if tokens.next().is_none() => {
                Some(SlashCommand::Reasoning(Some(toggle.to_string())))
            }
            _ => None,
        },
        "/usage" => match tokens.next() {
            None => Some(SlashCommand::Usage(None)),
            Some(toggle) if tokens.next().is_none() => {
                Some(SlashCommand::Usage(Some(toggle.to_string())))
            }
            _ => None,
        },
        "/tools" => match tokens.next() {
            None => Some(SlashCommand::Tools(None)),
            Some(toggle) if tokens.next().is_none() => {
                Some(SlashCommand::Tools(Some(toggle.to_string())))
            }
            _ => None,
        },
        "/display" if tokens.next().is_none() => Some(SlashCommand::Display),
        "/recap" if tokens.next().is_none() => Some(SlashCommand::Recap),
        "/steer" => {
            let args: Vec<&str> = tokens.collect();
            let (node, rest) = split_node_flag(&args)?;
            if rest.is_empty() {
                None
            } else {
                Some(SlashCommand::Steer {
                    node,
                    text: rest.join(" "),
                })
            }
        }
        "/followup" | "/follow-up" => {
            let args: Vec<&str> = tokens.collect();
            let (node, rest) = split_node_flag(&args)?;
            if rest.is_empty() {
                None
            } else {
                Some(SlashCommand::Followup {
                    node,
                    text: rest.join(" "),
                })
            }
        }
        "/append-mode" => {
            let args: Vec<&str> = tokens.collect();
            let (node, rest) = split_node_flag(&args)?;
            match rest.len() {
                0 => Some(SlashCommand::AppendMode { node, mode: None }),
                1 => Some(SlashCommand::AppendMode {
                    node,
                    mode: Some(rest[0].clone()),
                }),
                _ => None,
            }
        }
        "/convert-pending" | "/steer-pending" => {
            let args: Vec<&str> = tokens.collect();
            let (node, rest) = split_node_flag(&args)?;
            if rest.is_empty() {
                Some(SlashCommand::ConvertPending { node })
            } else {
                None
            }
        }
        "/loops" => {
            let args: Vec<&str> = tokens.collect();
            match args.len() {
                0 => Some(SlashCommand::Loops(None)),
                1 => Some(SlashCommand::Loops(Some(args[0].to_string()))),
                _ => None,
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_commands() {
        assert_eq!(parse_command("/quit"), Some(SlashCommand::Quit));
        assert_eq!(parse_command("/exit"), Some(SlashCommand::Quit));
        assert_eq!(parse_command("/help"), Some(SlashCommand::Help));
        assert_eq!(parse_command("/clear"), Some(SlashCommand::Clear));
        assert_eq!(parse_command("/new"), Some(SlashCommand::New));
        assert_eq!(
            parse_command("/checkpoints"),
            Some(SlashCommand::Checkpoints)
        );
        assert_eq!(parse_command("/history"), Some(SlashCommand::History(None)));
        assert_eq!(
            parse_command("/history cargo"),
            Some(SlashCommand::History(Some("cargo".to_string())))
        );
        assert_eq!(parse_command("/history a b"), None);
        assert_eq!(parse_command("/rerun"), Some(SlashCommand::Rerun(None)));
        assert_eq!(
            parse_command("/rerun 3"),
            Some(SlashCommand::Rerun(Some(3)))
        );
        assert_eq!(parse_command("/rerun 0"), None);
        assert_eq!(parse_command("/rerun abc"), None);
        assert_eq!(parse_command("/rerun 1 2"), None);
        assert_eq!(parse_command("/quit now"), None);
        assert_eq!(parse_command("/recap"), Some(SlashCommand::Recap));
        assert_eq!(parse_command("/recap now"), None);
        assert_eq!(parse_command("hello"), None);
        assert_eq!(parse_command("/unknown"), None);
    }

    #[test]
    fn parses_display_commands() {
        assert_eq!(
            parse_command("/markdown"),
            Some(SlashCommand::Markdown(None))
        );
        assert_eq!(
            parse_command("/markdown plain"),
            Some(SlashCommand::Markdown(Some("plain".to_string())))
        );
        assert_eq!(parse_command("/markdown a b"), None);
        assert_eq!(
            parse_command("/reasoning off"),
            Some(SlashCommand::Reasoning(Some("off".to_string())))
        );
        assert_eq!(parse_command("/usage"), Some(SlashCommand::Usage(None)));
        assert_eq!(
            parse_command("/tools on"),
            Some(SlashCommand::Tools(Some("on".to_string())))
        );
        assert_eq!(parse_command("/display"), Some(SlashCommand::Display));
        assert_eq!(parse_command("/display x"), None);
    }

    #[test]
    fn parses_nested_loop_commands() {
        assert_eq!(
            parse_command("/steer hold on"),
            Some(SlashCommand::Steer {
                node: None,
                text: "hold on".to_string(),
            })
        );
        assert_eq!(
            parse_command("/steer --node worker-1 hold on"),
            Some(SlashCommand::Steer {
                node: Some("worker-1".to_string()),
                text: "hold on".to_string(),
            })
        );
        assert_eq!(
            parse_command("/followup --node=worker-1 retry"),
            Some(SlashCommand::Followup {
                node: Some("worker-1".to_string()),
                text: "retry".to_string(),
            })
        );
        assert_eq!(parse_command("/steer"), None);
        assert_eq!(parse_command("/steer --node"), None);
        assert_eq!(
            parse_command("/append-mode steering"),
            Some(SlashCommand::AppendMode {
                node: None,
                mode: Some("steering".to_string()),
            })
        );
        assert_eq!(
            parse_command("/append-mode --node worker-1 followup"),
            Some(SlashCommand::AppendMode {
                node: Some("worker-1".to_string()),
                mode: Some("followup".to_string()),
            })
        );
        assert_eq!(
            parse_command("/convert-pending"),
            Some(SlashCommand::ConvertPending { node: None })
        );
        assert_eq!(
            parse_command("/convert-pending --node worker-1"),
            Some(SlashCommand::ConvertPending {
                node: Some("worker-1".to_string()),
            })
        );
        assert_eq!(parse_command("/convert-pending extra"), None);
        assert_eq!(parse_command("/loops"), Some(SlashCommand::Loops(None)));
        assert_eq!(
            parse_command("/loops exec-1"),
            Some(SlashCommand::Loops(Some("exec-1".to_string())))
        );
        assert_eq!(parse_command("/loops a b"), None);
    }
}
