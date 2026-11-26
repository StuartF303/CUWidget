use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Window {
    pub utilization: f64,
    pub resets_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageSnapshot {
    pub five_hour: Option<Window>,
    pub seven_day: Option<Window>,
    pub fetched_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ParseError {
    #[error("usage response is not a JSON object")]
    NotAnObject,
}

fn parse_window(v: Option<&serde_json::Value>) -> Option<Window> {
    let obj = v?.as_object()?;
    let utilization = obj.get("utilization")?.as_f64()?;
    let resets_at = obj
        .get("resets_at")
        .and_then(|r| r.as_str())
        .and_then(|r| DateTime::parse_from_rfc3339(r).ok())
        .map(|d| d.with_timezone(&Utc));
    Some(Window { utilization, resets_at })
}

/// Leniently parse the usage endpoint response. Unknown keys are ignored; a
/// missing, null or malformed window becomes `None`. Only input that is not a
/// JSON object is an error.
pub fn parse_usage(json: &str, fetched_at: DateTime<Utc>) -> Result<UsageSnapshot, ParseError> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|_| ParseError::NotAnObject)?;
    if !value.is_object() {
        return Err(ParseError::NotAnObject);
    }
    Ok(UsageSnapshot {
        five_hour: parse_window(value.get("five_hour")),
        seven_day: parse_window(value.get("seven_day")),
        fetched_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t0() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 22, 4, 0, 0).unwrap()
    }
    fn reset() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 22, 9, 0, 0).unwrap()
    }

    #[test]
    fn full_response() {
        let j = r#"{"five_hour":{"utilization":8.0,"resets_at":"2026-01-22T09:00:00Z"},
                    "seven_day":{"utilization":77.0,"resets_at":"2026-01-25T09:00:00Z"}}"#;
        let s = parse_usage(j, t0()).unwrap();
        assert_eq!(
            s.five_hour,
            Some(Window { utilization: 8.0, resets_at: Some(reset()) })
        );
        let sd = s.seven_day.unwrap();
        assert_eq!(sd.utilization, 77.0);
        assert_eq!(sd.resets_at, Some(Utc.with_ymd_and_hms(2026, 1, 25, 9, 0, 0).unwrap()));
        assert_eq!(s.fetched_at, t0());
    }

    #[test]
    fn unknown_fields_ignored() {
        let j = r#"{"five_hour":{"utilization":8.0,"resets_at":"2026-01-22T09:00:00Z","extra":1},
                    "seven_day_opus":{"utilization":1.0,"resets_at":null},"foo":[1,2]}"#;
        let s = parse_usage(j, t0()).unwrap();
        assert_eq!(s.five_hour.unwrap().utilization, 8.0);
        assert_eq!(s.seven_day, None);
    }

    #[test]
    fn null_or_missing_window_is_none() {
        let s = parse_usage(r#"{"five_hour":null,"seven_day":{"utilization":3.0,"resets_at":null}}"#, t0()).unwrap();
        assert_eq!(s.five_hour, None);
        assert!(s.seven_day.is_some());
        let s = parse_usage("{}", t0()).unwrap();
        assert_eq!((s.five_hour, s.seven_day), (None, None));
    }

    #[test]
    fn null_resets_at_is_none() {
        let s = parse_usage(r#"{"five_hour":{"utilization":8.0,"resets_at":null}}"#, t0()).unwrap();
        assert_eq!(s.five_hour.unwrap().resets_at, None);
    }

    #[test]
    fn unparseable_resets_at_is_none() {
        let s = parse_usage(r#"{"five_hour":{"utilization":8.0,"resets_at":"not a date"}}"#, t0()).unwrap();
        let w = s.five_hour.unwrap();
        assert_eq!(w.resets_at, None);
        assert_eq!(w.utilization, 8.0);
    }

    #[test]
    fn utilization_not_clamped() {
        let s = parse_usage(r#"{"five_hour":{"utilization":104.5,"resets_at":null}}"#, t0()).unwrap();
        assert_eq!(s.five_hour.unwrap().utilization, 104.5);
    }

    #[test]
    fn non_json_is_error() {
        assert!(parse_usage("hello", t0()).is_err());
        assert!(parse_usage("[1,2]", t0()).is_err());
        assert!(parse_usage("", t0()).is_err());
    }
}
