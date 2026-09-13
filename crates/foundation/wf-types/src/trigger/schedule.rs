//! Schedule (cron) and webhook producer specifications.
//!
//! The scheduler owns time and the gateway owns HTTP ingress; the types here
//! own the validated shape both sides agree on. Custom resources declare them
//! through `CustomTriggerCondition`; the registrar stores the validated spec
//! as JSON under [`SCHEDULE_SPEC_METADATA_KEY`] / [`WEBHOOK_SPEC_METADATA_KEY`]
//! in `TriggerTemplate.metadata`, and the scheduler / gateway read it back.
//! Conditions themselves still translate to `NODE_CUSTOM_EVENT` through
//! `TriggerSource::translate_schedule_to_condition` /
//! `translate_webhook_to_condition`, reusing the competition scope keys.
//!
//! Cron parsing is intentionally dependency-free (only `chrono`, already in
//! the tree): five fields `minute hour dom month dow` with `*`, `*/n`,
//! `a,b`, `a-b`, `a-b/n`, numbers and month/weekday names. Timezone support
//! covers UTC, local time and fixed offsets (`+08:00`); IANA names are
//! rejected with an explicit error until a timezone database crate lands.

use chrono::{DateTime, Datelike, FixedOffset, Local, TimeZone, Timelike, Utc};
use serde::{Deserialize, Serialize};

/// Metadata key holding the validated [`ScheduleSpec`] JSON of a template.
pub const SCHEDULE_SPEC_METADATA_KEY: &str = "schedule_spec";
/// Metadata key holding the validated [`WebhookSpec`] JSON of a template.
pub const WEBHOOK_SPEC_METADATA_KEY: &str = "webhook_spec";
/// Metadata key carrying the scheduler/webhook fire identity for idempotency
/// and for the execution-creating quota key (`<name>:<fire_id>`).
pub const FIRE_ID_METADATA_KEY: &str = "fire_id";
/// Metadata key marking the producer of a custom event (`schedule`/`webhook`).
pub const PRODUCER_SOURCE_METADATA_KEY: &str = "source";

/// What a missed tick (scheduler down across the fire time) does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleMisfirePolicy {
    /// Drop expired ticks.
    #[default]
    Skip,
    /// Run once immediately for all missed ticks.
    FireOnce,
    /// Replay every missed tick (bounded by the dispatch burst cap).
    FireAll,
}

/// Where a schedule tick is routed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScheduleTarget {
    /// The tick names a live execution resolved at fire time through the
    /// runtime timer-binding table (`schedule_name -> execution_id`).
    ExecutionScoped,
    /// The tick cold-starts a fresh run; exactly one of `workflow_id` /
    /// `agent_id` must be set.
    Create {
        #[serde(skip_serializing_if = "Option::is_none")]
        workflow_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        agent_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        input: Option<serde_json::Value>,
    },
}

impl ScheduleTarget {
    /// Whether this target needs an execution-creating trigger action.
    pub fn is_creating(&self) -> bool {
        matches!(self, Self::Create { .. })
    }

    /// Validate the target shape, naming the owning trigger for errors.
    pub fn validate(&self, trigger_name: &str) -> Result<(), String> {
        match self {
            Self::ExecutionScoped => Ok(()),
            Self::Create {
                workflow_id,
                agent_id,
                ..
            } => match (workflow_id, agent_id) {
                (Some(w), None) if !w.is_empty() => Ok(()),
                (None, Some(a)) if !a.is_empty() => Ok(()),
                _ => Err(format!(
                    "trigger '{}' targets creation but sets neither a workflow_id nor an agent_id (exactly one is required)",
                    trigger_name
                )),
            },
        }
    }
}

/// Validated schedule producer specification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScheduleSpec {
    pub cron: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tz: Option<String>,
    #[serde(default)]
    pub target: ScheduleTarget,
    #[serde(default)]
    pub misfire: ScheduleMisfirePolicy,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

impl Default for ScheduleTarget {
    fn default() -> Self {
        Self::ExecutionScoped
    }
}

impl ScheduleSpec {
    /// Validate cron, timezone and target; the parsed schedule is returned
    /// for callers that tick immediately after validation.
    pub fn validate(&self, trigger_name: &str) -> Result<CronSchedule, String> {
        let parsed = CronSchedule::parse(&self.cron).map_err(|e| {
            format!(
                "trigger '{}' has an invalid cron expression '{}': {}",
                trigger_name, self.cron, e
            )
        })?;
        parse_tz_offset(self.tz.as_deref()).map_err(|e| {
            format!(
                "trigger '{}' has an invalid timezone '{}': {}",
                trigger_name,
                self.tz.as_deref().unwrap_or_default(),
                e
            )
        })?;
        self.target.validate(trigger_name)?;
        Ok(parsed)
    }
}

/// Webhook authentication model: per-hook independent secret (a central
/// preset only supplies rate/body defaults, never a default key).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum WebhookAuth {
    /// No authentication (explicit opt-in; suitable behind an authenticated
    /// reverse proxy only).
    #[default]
    None,
    /// Bearer/`x-hook-token` shared secret.
    Token { token: String },
}

/// Validated webhook ingress specification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebhookSpec {
    /// Mounted as `POST /api/v1/hooks/{name}`; globally unique across
    /// templates (validated at registration).
    pub path: String,
    #[serde(default)]
    pub auth: WebhookAuth,
    /// Body keys copied into the published event metadata (absent copies the
    /// whole object body when it fits the metadata budget).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_mapping: Option<Vec<String>>,
    #[serde(default)]
    pub target: ScheduleTarget,
}

impl WebhookSpec {
    /// Validate path shape, auth completeness and target.
    pub fn validate(&self, trigger_name: &str) -> Result<(), String> {
        if self.path.is_empty() || !self.path.starts_with('/') {
            return Err(format!(
                "trigger '{}' has an invalid webhook path '{}': must start with '/'",
                trigger_name, self.path
            ));
        }
        if let WebhookAuth::Token { token } = &self.auth {
            if token.is_empty() {
                return Err(format!(
                    "trigger '{}' declares token auth with an empty token",
                    trigger_name
                ));
            }
        }
        self.target.validate(trigger_name)?;
        Ok(())
    }
}

/// Resolve a schedule `tz` value to a fixed offset. Accepts absent (UTC),
/// `UTC`, `local` and `+HH:MM` / `-HHMM` style offsets. IANA names are
/// rejected explicitly until a timezone database crate is available.
pub fn parse_tz_offset(tz: Option<&str>) -> Result<FixedOffset, String> {
    match tz {
        None => Ok(FixedOffset::east_opt(0).expect("zero offset")),
        Some(name) => {
            let lower = name.to_lowercase();
            if lower == "utc" || lower == "z" {
                return Ok(FixedOffset::east_opt(0).expect("zero offset"));
            }
            if lower == "local" {
                return Ok(Local::now().offset().to_owned());
            }
            parse_fixed_offset(name).ok_or_else(|| {
                format!(
                    "unsupported timezone '{}': use UTC, local or a fixed offset like +08:00 (IANA names need a timezone database crate first)",
                    name
                )
            })
        }
    }
}

fn parse_fixed_offset(name: &str) -> Option<FixedOffset> {
    let (sign, rest) = match name.strip_prefix('+') {
        Some(rest) => (1, rest),
        None => (-1, name.strip_prefix('-')?),
    };
    let (hours, minutes) = match rest.split_once(':') {
        Some((h, m)) => (h.parse::<i32>().ok()?, m.parse::<i32>().ok()?),
        None if rest.len() == 4 => (
            rest[..2].parse::<i32>().ok()?,
            rest[2..].parse::<i32>().ok()?,
        ),
        None if rest.len() == 2 => (rest.parse::<i32>().ok()?, 0),
        _ => return None,
    };
    if !(0..=23).contains(&hours) || !(0..=59).contains(&minutes) {
        return None;
    }
    FixedOffset::east_opt(sign * (hours * 3600 + minutes * 60))
}

/// Parsed five-field cron schedule (`minute hour dom month dow`).
#[derive(Debug, Clone)]
pub struct CronSchedule {
    minutes: Vec<bool>,
    hours: Vec<bool>,
    days: Vec<bool>,
    months: Vec<bool>,
    weekdays: Vec<bool>,
    dom_restricted: bool,
    dow_restricted: bool,
}

/// Upper bound on the minute-by-minute search for the next fire time
/// (366 days); a schedule that never fires within a leap year is treated as
/// unsatisfiable.
pub const CRON_SEARCH_LIMIT_MINUTES: u64 = 366 * 24 * 60;

impl CronSchedule {
    /// Parse a five-field cron expression.
    pub fn parse(expression: &str) -> Result<Self, String> {
        let fields: Vec<&str> = expression.split_whitespace().collect();
        if fields.len() != 5 {
            return Err(format!(
                "expected 5 fields (minute hour dom month dow), got {}",
                fields.len()
            ));
        }
        let minutes = parse_field(fields[0], 0, 59, &[])?;
        let hours = parse_field(fields[1], 0, 23, &[])?;
        let days = parse_field(fields[2], 1, 31, &[])?;
        let months = parse_field(fields[3], 1, 12, &MONTH_NAMES)?;
        let weekdays = parse_field(fields[4], 0, 7, &WEEKDAY_NAMES)?;
        let dom_restricted = fields[2] != "*";
        let dow_restricted = fields[4] != "*";
        // Cron allows 7 as Sunday; fold it into 0.
        let mut weekdays = weekdays;
        if weekdays.len() == 8 {
            weekdays[0] = weekdays[0] || weekdays[7];
            weekdays.pop();
        }
        Ok(Self {
            minutes,
            hours,
            days,
            months,
            weekdays,
            dom_restricted,
            dow_restricted,
        })
    }

    /// Whether a unix timestamp (seconds) matches in the given offset.
    fn matches(&self, timestamp: i64, offset: &FixedOffset) -> bool {
        let local = offset.timestamp_opt(timestamp, 0).unwrap();
        let minute = local.minute() as usize;
        let hour = local.hour() as usize;
        let day = local.day() as usize;
        let month = local.month() as usize;
        let weekday = local.weekday().num_days_from_sunday() as usize;
        if !self.minutes[minute] || !self.hours[hour] || !self.months[month] {
            return false;
        }
        // Standard cron day semantics: when both dom and dow are restricted,
        // either matching fires; otherwise the restricted one (or the
        // unrestricted wildcard) decides.
        let day_hit = match (self.dom_restricted, self.dow_restricted) {
            (true, true) => self.days[day] || self.weekdays[weekday],
            (true, false) => self.days[day],
            (false, true) => self.weekdays[weekday],
            (false, false) => true,
        };
        day_hit
    }

    /// First fire time strictly after `after` (interpreted in `tz_name`).
    pub fn next_after(
        &self,
        after: DateTime<Utc>,
        tz_name: Option<&str>,
    ) -> Result<DateTime<Utc>, String> {
        let offset = parse_tz_offset(tz_name)?;
        // Truncate to the minute and step past `after`.
        let mut cursor = (after.timestamp() / 60) * 60 + 60;
        for _ in 0..CRON_SEARCH_LIMIT_MINUTES {
            if self.matches(cursor, &offset) {
                return Ok(DateTime::from_timestamp(cursor, 0)
                    .expect("search stays in range")
                    .to_utc());
            }
            cursor += 60;
        }
        Err("schedule never fires within 366 days; check the day/month combination".to_string())
    }
}

const MONTH_NAMES: [(&str, u32); 12] = [
    ("jan", 1),
    ("feb", 2),
    ("mar", 3),
    ("apr", 4),
    ("may", 5),
    ("jun", 6),
    ("jul", 7),
    ("aug", 8),
    ("sep", 9),
    ("oct", 10),
    ("nov", 11),
    ("dec", 12),
];

const WEEKDAY_NAMES: [(&str, u32); 7] = [
    ("sun", 0),
    ("mon", 1),
    ("tue", 2),
    ("wed", 3),
    ("thu", 4),
    ("fri", 5),
    ("sat", 6),
];

fn parse_field(
    field: &str,
    min: u32,
    max: u32,
    names: &[(&str, u32)],
) -> Result<Vec<bool>, String> {
    // Sundays may be written as 7; size the table accordingly.
    let size = (max + 1) as usize;
    let mut bits = vec![false; size];
    let resolve = |token: &str| -> Result<u32, String> {
        let lower = token.to_lowercase();
        if let Some((_, value)) = names.iter().find(|(name, _)| *name == lower) {
            return Ok(*value);
        }
        token.parse::<u32>().map_err(|_| {
            format!(
                "invalid cron value '{}' (expected {}-{} or a name)",
                token, min, max
            )
        })
    };
    for part in field.split(',') {
        let (range, step) = match part.split_once('/') {
            Some((range, step)) => {
                let step: u32 = step
                    .parse()
                    .map_err(|_| format!("invalid cron step '{}'", step))?;
                if step == 0 {
                    return Err("cron step must be at least 1".to_string());
                }
                (range, step)
            }
            None => (part, 1),
        };
        let (lo, hi) = if range == "*" || range.is_empty() {
            (min, max)
        } else if let Some((start, end)) = range.split_once('-') {
            let lo = resolve(start)?;
            let hi = resolve(end)?;
            if lo > hi {
                return Err(format!("invalid cron range '{}'", range));
            }
            (lo, hi)
        } else {
            let value = resolve(range)?;
            (value, value)
        };
        if lo < min || hi > max {
            return Err(format!(
                "cron value out of range (expected {}-{}, got {}-{})",
                min, max, lo, hi
            ));
        }
        let mut value = lo;
        while value <= hi {
            if (value - lo) % step == 0 {
                bits[value as usize] = true;
            }
            value += 1;
        }
    }
    if !bits.iter().any(|hit| *hit) {
        return Err("cron field matches nothing".to_string());
    }
    Ok(bits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn every_minute_matches_next_minute() {
        let schedule = CronSchedule::parse("* * * * *").unwrap();
        let after = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 30).unwrap();
        let next = schedule.next_after(after, None).unwrap();
        assert_eq!(next, Utc.with_ymd_and_hms(2026, 1, 1, 0, 1, 0).unwrap());
    }

    #[test]
    fn nightly_two_am_utc() {
        let schedule = CronSchedule::parse("0 2 * * *").unwrap();
        let after = Utc.with_ymd_and_hms(2026, 5, 4, 3, 0, 0).unwrap();
        let next = schedule.next_after(after, None).unwrap();
        assert_eq!(next, Utc.with_ymd_and_hms(2026, 5, 5, 2, 0, 0).unwrap());
    }

    #[test]
    fn fixed_offset_shifts_fire_time() {
        let schedule = CronSchedule::parse("0 9 * * *").unwrap();
        let after = Utc.with_ymd_and_hms(2026, 5, 4, 0, 0, 0).unwrap();
        // 09:00 at +08:00 is 01:00 UTC the same day.
        let next = schedule.next_after(after, Some("+08:00")).unwrap();
        assert_eq!(next, Utc.with_ymd_and_hms(2026, 5, 4, 1, 0, 0).unwrap());
    }

    #[test]
    fn weekday_names_and_steps() {
        let schedule = CronSchedule::parse("*/15 9-17 * * mon-fri").unwrap();
        // Saturday 2026-05-09 12:00 UTC -> Monday 2026-05-11 09:00 UTC.
        let after = Utc.with_ymd_and_hms(2026, 5, 9, 12, 0, 0).unwrap();
        let next = schedule.next_after(after, None).unwrap();
        assert_eq!(next, Utc.with_ymd_and_hms(2026, 5, 11, 9, 0, 0).unwrap());
    }

    #[test]
    fn dom_dow_restricted_match_either() {
        // Fires on the 1st of the month or on Mondays at midnight.
        let schedule = CronSchedule::parse("0 0 1 * mon").unwrap();
        // Tuesday 2026-05-05 -> next Monday 2026-05-11 (the 1st already passed).
        let after = Utc.with_ymd_and_hms(2026, 5, 5, 0, 0, 1).unwrap();
        let next = schedule.next_after(after, None).unwrap();
        assert_eq!(next, Utc.with_ymd_and_hms(2026, 5, 11, 0, 0, 0).unwrap());
    }

    #[test]
    fn invalid_expressions_rejected() {
        assert!(CronSchedule::parse("* * * *").is_err());
        assert!(CronSchedule::parse("61 * * * *").is_err());
        assert!(CronSchedule::parse("*/0 * * * *").is_err());
        assert!(
            CronSchedule::parse("0 0 30 2 *").is_ok(),
            "parsed; unsatisfiable only at tick time"
        );
    }

    #[test]
    fn unsatisfiable_schedule_errors_at_tick_time() {
        let schedule = CronSchedule::parse("0 0 30 2 *").unwrap();
        let after = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        assert!(schedule.next_after(after, None).is_err());
    }

    #[test]
    fn timezone_resolution() {
        assert!(parse_tz_offset(None).is_ok());
        assert!(parse_tz_offset(Some("UTC")).is_ok());
        assert!(parse_tz_offset(Some("+08:00")).is_ok());
        assert!(parse_tz_offset(Some("-0500")).is_ok());
        assert!(parse_tz_offset(Some("local")).is_ok());
        let iana = parse_tz_offset(Some("Asia/Shanghai"));
        assert!(iana.is_err(), "IANA names must fail loudly for now");
        assert!(iana.unwrap_err().contains("timezone database"));
    }

    #[test]
    fn creation_target_requires_exactly_one_backend() {
        let good = ScheduleTarget::Create {
            workflow_id: Some("w".to_string()),
            agent_id: None,
            input: None,
        };
        assert!(good.validate("t").is_ok());
        let bad = ScheduleTarget::Create {
            workflow_id: Some("w".to_string()),
            agent_id: Some("a".to_string()),
            input: None,
        };
        assert!(bad.validate("t").is_err());
        let empty = ScheduleTarget::Create {
            workflow_id: None,
            agent_id: None,
            input: None,
        };
        assert!(empty.validate("t").is_err());
    }

    #[test]
    fn webhook_spec_validation() {
        let spec = WebhookSpec {
            path: "/hooks/deploy".to_string(),
            auth: WebhookAuth::Token {
                token: "secret".to_string(),
            },
            input_mapping: None,
            target: ScheduleTarget::ExecutionScoped,
        };
        assert!(spec.validate("hook").is_ok());
        let bad = WebhookSpec {
            path: "hooks/deploy".to_string(),
            ..spec.clone()
        };
        assert!(bad.validate("hook").is_err());
    }
}
