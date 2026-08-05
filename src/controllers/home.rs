use std::collections::HashMap;

use chrono::{Datelike, NaiveDate, Weekday};
use loco_rs::prelude::*;

use crate::models::_entities::events::{Entity as Events, Model as Event};

fn days_in_month(year: i32, month: u32) -> u32 {
    let (next_year, next_month) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .unwrap()
        .signed_duration_since(NaiveDate::from_ymd_opt(year, month, 1).unwrap())
        .num_days() as u32
}

fn weekday_letter(weekday: Weekday) -> &'static str {
    match weekday {
        Weekday::Mon => "M",
        Weekday::Tue => "T",
        Weekday::Wed => "W",
        Weekday::Thu => "T",
        Weekday::Fri => "F",
        Weekday::Sat => "S",
        Weekday::Sun => "S",
    }
}

/// Colors an event by category: events labeled "holiday" are green
/// regardless of length, otherwise single-day events are blue and events
/// spanning two or more days are orange.
fn event_color(event: &Event) -> &'static str {
    let is_holiday = event
        .title
        .as_deref()
        .is_some_and(|title| title.to_lowercase().contains("holiday"));
    if is_holiday {
        return "#2e7d32";
    }

    let is_multi_day = event
        .start
        .zip(event.end)
        .is_some_and(|(start, end)| (end - start).num_days() >= 2);
    if is_multi_day {
        "#e67e22"
    } else {
        "#3b82f6"
    }
}

/// Buckets events by the calendar days they occur on. `end` is treated as
/// exclusive (matching the ICS all-day convention already used when events
/// are synced), so a single-day event's `end` equal to `start + 1` lands
/// only on `start`, while multi-day events span every day up to (but not
/// including) `end`.
fn bucket_events_by_day(events: &[Event]) -> HashMap<NaiveDate, Vec<&Event>> {
    let mut by_day: HashMap<NaiveDate, Vec<&Event>> = HashMap::new();

    for event in events {
        let Some(start) = event.start else { continue };
        let end = event.end.filter(|e| *e > start).unwrap_or(start.succ_opt().unwrap_or(start));

        let mut day = start;
        while day < end {
            by_day.entry(day).or_default().push(event);
            day = day.succ_opt().unwrap_or(day);
            if day == start {
                break; // guard against a date overflow leaving `day` unchanged
            }
        }
    }

    by_day
}

async fn index(State(ctx): State<AppContext>, ViewEngine(v): ViewEngine<TeraView>) -> Result<Response> {
    let today = chrono::Local::now().date_naive();
    let year = today.year();

    let events = Events::find().all(&ctx.db).await?;
    let events_by_day = bucket_events_by_day(&events);

    let months: Vec<_> = (1..=12u32)
        .map(|month| {
            let name = NaiveDate::from_ymd_opt(year, month, 1)
                .unwrap()
                .format("%b")
                .to_string();
            let days: Vec<_> = (1..=days_in_month(year, month))
                .map(|day| {
                    let date = NaiveDate::from_ymd_opt(year, month, day).unwrap();
                    let weekday = date.weekday();
                    let day_events = events_by_day.get(&date);
                    let label = day_events
                        .map(|day_events| {
                            day_events
                                .iter()
                                .filter_map(|event| event.title.as_deref())
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_default();
                    let color = day_events
                        .and_then(|day_events| day_events.first())
                        .map(|event| event_color(event));
                    data!({
                        "day": day,
                        "weekday": weekday_letter(weekday),
                        "label": label,
                        "color": color,
                        "is_today": date == today,
                        "is_past": date < today,
                    })
                })
                .collect();
            data!({"name": name, "days": days})
        })
        .collect();

    format::render().view(&v, "home/hello.html", data!({"year": year, "months": months}))
}

pub fn routes() -> Routes {
    Routes::new().add("/", get(index))
}
