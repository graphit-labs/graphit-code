use super::*;
use crate::task_spec::FieldUpdate;
use serde::Deserialize;

fn day(value: &str) -> CalendarDate {
    value.parse().unwrap()
}

#[test]
fn calendar_validation_and_canonical_roundtrip_use_real_days() {
    for value in [
        "0000-02-29",
        "2000-02-29",
        "2024-02-29",
        "2024-12-31",
        "2025-01-01",
        "9999-12-31",
    ] {
        let parsed = day(value);
        assert_eq!(parsed.to_string(), value);
        assert_eq!(
            serde_json::to_string(&parsed).unwrap(),
            format!("\"{value}\"")
        );
        assert_eq!(
            serde_json::from_value::<CalendarDate>(serde_json::json!(value)).unwrap(),
            parsed
        );
    }
    for value in [
        "2023-02-29",
        "2100-02-29",
        "2024-04-31",
        "2024-00-01",
        "2024-13-01",
        "2024-01-00",
        "2024-01-32",
    ] {
        let error = value.parse::<CalendarDate>().unwrap_err();
        assert!(
            matches!(error, CalendarDateError::InvalidCalendarDay(_)),
            "{value}"
        );
        assert!(error.source().is_some());
    }
}

#[test]
fn shape_rejects_normalization_signed_or_wide_year_and_non_ascii() {
    for value in [
        "",
        " 2024-01-01",
        "2024-01-01 ",
        "2024-1-01",
        "2024-01-1",
        "+2024-01-01",
        "-0001-01-01",
        "10000-01-01",
        "2024/01/01",
        "２０２４-01-01",
        "2024-01-01T00:00:00Z",
        "2024-01-01\n",
    ] {
        assert!(
            matches!(
                value.parse::<CalendarDate>(),
                Err(CalendarDateError::NoncanonicalFormat)
            ),
            "{value}"
        );
    }
    for value in [
        serde_json::json!(0),
        serde_json::json!(null),
        serde_json::json!(true),
        serde_json::json!([]),
        serde_json::json!({}),
    ] {
        assert!(serde_json::from_value::<CalendarDate>(value).is_err());
    }
}

#[test]
fn intervals_are_inclusive_across_month_year_and_leap_boundaries() {
    for (start, end, duration) in [
        ("2024-02-29", "2024-02-29", 1),
        ("2024-02-28", "2024-03-01", 3),
        ("2023-02-28", "2023-03-01", 2),
        ("2024-12-31", "2025-01-01", 2),
        ("0000-01-01", "9999-12-31", 3_652_425),
    ] {
        let schedule = TaskSchedule::new(Some(day(start)), Some(day(end)), None).unwrap();
        assert_eq!(schedule.inclusive_duration_days(), Some(duration));
        assert_eq!(schedule.start_date(), Some(day(start)));
        assert_eq!(schedule.end_date(), Some(day(end)));
    }
}

#[test]
fn partial_and_due_only_schedules_have_no_invented_duration() {
    let date = day("2024-02-29");
    for fields in [
        (Some(date), None, None),
        (None, Some(date), None),
        (None, None, Some(date)),
        (Some(date), None, Some(date)),
        (None, Some(date), Some(date)),
    ] {
        let schedule = TaskSchedule::new(fields.0, fields.1, fields.2).unwrap();
        assert_eq!(schedule.inclusive_duration_days(), None);
    }
    for due in ["2023-01-01", "2025-01-01"] {
        let schedule = TaskSchedule::new(Some(date), Some(date), Some(day(due))).unwrap();
        assert_eq!(schedule.due_date(), Some(day(due)));
        assert_eq!(schedule.inclusive_duration_days(), Some(1));
    }
}

#[test]
fn constructor_rejects_empty_and_inverted_schedule() {
    assert_eq!(
        TaskSchedule::new(None, None, None),
        Err(TaskScheduleError::Empty)
    );
    assert_eq!(
        TaskSchedule::new(
            Some(day("2025-01-01")),
            Some(day("2024-12-31")),
            Some(day("2025-02-01"))
        ),
        Err(TaskScheduleError::InvertedInterval)
    );
}

#[test]
fn schedule_codec_accepts_partial_nulls_and_serializes_only_real_dates() {
    let schedule: TaskSchedule =
        serde_json::from_str(r#"{"start_date":null,"end_date":"2024-02-29","due_date":null}"#)
            .unwrap();
    assert_eq!(
        schedule,
        TaskSchedule::new(None, Some(day("2024-02-29")), None).unwrap()
    );
    assert_eq!(
        serde_json::to_string(&schedule).unwrap(),
        r#"{"end_date":"2024-02-29"}"#
    );
    let all: TaskSchedule = serde_json::from_str(
        r#"{"start_date":"2024-02-28","end_date":"2024-03-01","due_date":"2023-01-01"}"#,
    )
    .unwrap();
    assert_eq!(
        serde_json::from_str::<TaskSchedule>(&serde_json::to_string(&all).unwrap()).unwrap(),
        all
    );
}

#[test]
fn strict_json_rejects_unknown_duplicate_empty_invalid_and_inverted_fields() {
    for value in [
        r#"{}"#,
        r#"null"#,
        r#"[]"#,
        r#""2024-01-01""#,
        r#"{"start_date":null,"end_date":null,"due_date":null}"#,
        r#"{"start_date":""}"#,
        r#"{"due_date":"2023-02-29"}"#,
        r#"{"start_date":"2024-01-02","end_date":"2024-01-01"}"#,
        r#"{"start_date":"2024-01-01","other":"2024-01-02"}"#,
        r#"{"due_date":"2024-01-01","due_date":"2024-01-02"}"#,
        r#"{"start_date":null,"start_date":"2024-01-01"}"#,
        r#"{"end_date":"2024-01-01","end_date":null}"#,
        r#"{"due_date":123}"#,
    ] {
        assert!(
            serde_json::from_str::<TaskSchedule>(value).is_err(),
            "{value}"
        );
    }
    let error = serde_json::from_str::<TaskSchedule>(
        r#"{"start_date":"2024-01-02","end_date":"2024-01-01"}"#,
    )
    .unwrap_err();
    assert!(error.to_string().contains("end_date precedes start_date"));
}

#[test]
fn typed_serde_fields_distinguish_create_unscheduled_and_revision_keep_clear_replace() {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Create {
        schedule: Option<TaskSchedule>,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Revise {
        #[serde(default)]
        schedule: FieldUpdate<Option<TaskSchedule>>,
    }
    for input in ["{}", r#"{"schedule":null}"#] {
        assert!(
            serde_json::from_str::<Create>(input)
                .unwrap()
                .schedule
                .is_none()
        );
    }
    assert_eq!(
        serde_json::from_str::<Revise>("{}").unwrap().schedule,
        FieldUpdate::Keep
    );
    assert_eq!(
        serde_json::from_str::<Revise>(r#"{"schedule":null}"#)
            .unwrap()
            .schedule,
        FieldUpdate::Set(None)
    );
    let input = r#"{"schedule":{"due_date":"2024-02-29"}}"#;
    let expected = TaskSchedule::new(None, None, Some(day("2024-02-29"))).unwrap();
    assert_eq!(
        serde_json::from_str::<Create>(input).unwrap().schedule,
        Some(expected)
    );
    assert_eq!(
        serde_json::from_str::<Revise>(input).unwrap().schedule,
        FieldUpdate::Set(Some(expected))
    );
    for input in [
        r#"{"schedule":{}}"#,
        r#"{"schedule":{"start_date":null}}"#,
        r#"{"schedule":null,"schedule":null}"#,
    ] {
        assert!(serde_json::from_str::<Create>(input).is_err());
        assert!(serde_json::from_str::<Revise>(input).is_err());
    }
}
