use std::time::Duration;

use ical::{
    parser::ical::component::{IcalCalendar, IcalEvent},
    property::Property,
    IcalParser,
};
use loco_rs::prelude::*;
use tracing::{error, info, warn};

use crate::models::_entities::events::{ActiveModel, Column, Entity as Events};

const SYNC_INTERVAL: Duration = Duration::from_secs(5 * 60);

struct ParsedEvent {
    uid: String,
    title: Option<String>,
    start: Option<Date>,
    end: Option<Date>,
    color: Option<String>,
}

/// Spawns a `Tokio` task that periodically fetches the ICS feed at `url` and
/// syncs it into the `events` table (create/update/delete) for as long as
/// the process is running. Runs once immediately, then every
/// [`SYNC_INTERVAL`].
pub fn spawn(ctx: AppContext, url: String) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(SYNC_INTERVAL);
        loop {
            ticker.tick().await;
            match sync_once(&ctx, &url).await {
                Ok((created, updated, deleted)) => {
                    if created + updated + deleted > 0 {
                        info!(created, updated, deleted, "ics calendar sync complete");
                    }
                }
                Err(err) => error!(error = %err, "ics calendar sync failed"),
            }
        }
    });
}

async fn sync_once(ctx: &AppContext, url: &str) -> Result<(usize, usize, usize)> {
    let response = reqwest::get(url)
        .await
        .map_err(|err| Error::Message(format!("failed to fetch ICS feed: {err}")))?;
    let body = response
        .error_for_status()
        .map_err(|err| Error::Message(format!("ICS feed returned an error status: {err}")))?
        .text()
        .await
        .map_err(|err| Error::Message(format!("failed to read ICS feed body: {err}")))?;

    let parsed = parse_calendar(&body)?;
    upsert_and_prune(&ctx.db, parsed).await
}

fn parse_calendar(body: &str) -> Result<Vec<ParsedEvent>> {
    let mut events = Vec::new();

    for calendar in IcalParser::new(body.as_bytes()) {
        let calendar: IcalCalendar =
            calendar.map_err(|err| Error::Message(format!("failed to parse ICS feed: {err}")))?;

        let color = find_property(&calendar.properties, "X-APPLE-CALENDAR-COLOR")
            .map(|value| value.chars().take(7).collect::<String>());

        for event in &calendar.events {
            match parsed_event(event, color.as_deref()) {
                Some(parsed) => events.push(parsed),
                None => warn!("skipping ICS VEVENT without a UID"),
            }
        }
    }

    Ok(events)
}

fn parsed_event(event: &IcalEvent, color: Option<&str>) -> Option<ParsedEvent> {
    let uid = find_property(&event.properties, "UID")?.to_string();

    // Fastmail (and most calendar servers) simply drop deleted events from
    // the published feed, but honor an explicit cancellation too.
    if find_property(&event.properties, "STATUS") == Some("CANCELLED") {
        return None;
    }

    Some(ParsedEvent {
        uid,
        title: find_property(&event.properties, "SUMMARY").map(unescape_text),
        start: find_property(&event.properties, "DTSTART").and_then(parse_ics_date),
        end: find_property(&event.properties, "DTEND").and_then(parse_ics_date),
        color: color.map(str::to_string),
    })
}

fn find_property<'a>(properties: &'a [Property], name: &str) -> Option<&'a str> {
    properties
        .iter()
        .find(|property| property.name == name)
        .and_then(|property| property.value.as_deref())
}

/// Parses the date portion of a `DTSTART`/`DTEND` value. Handles both
/// `VALUE=DATE` (`20260722`) and `DATE-TIME` (`20260722T093000Z`) forms;
/// only the date component is kept since events render on a day grid.
fn parse_ics_date(value: &str) -> Option<Date> {
    let date_part = value.get(0..8)?;
    Date::parse_from_str(date_part, "%Y%m%d").ok()
}

fn unescape_text(value: &str) -> String {
    value
        .replace("\\n", "\n")
        .replace("\\N", "\n")
        .replace("\\,", ",")
        .replace("\\;", ";")
        .replace("\\\\", "\\")
}

async fn upsert_and_prune(
    db: &DatabaseConnection,
    parsed: Vec<ParsedEvent>,
) -> Result<(usize, usize, usize)> {
    if parsed.is_empty() {
        warn!("ics feed contained no events; skipping delete pass to avoid wiping the calendar");
        return Ok((0, 0, 0));
    }

    let txn = db.begin().await?;

    let mut created = 0usize;
    let mut updated = 0usize;
    let mut seen_uids = Vec::with_capacity(parsed.len());

    for event in parsed {
        seen_uids.push(event.uid.clone());

        let existing = Events::find()
            .filter(Column::Uid.eq(event.uid.clone()))
            .one(&txn)
            .await?;

        match existing {
            Some(model) => {
                if model.title != event.title
                    || model.start != event.start
                    || model.end != event.end
                    || model.color != event.color
                {
                    let mut active: ActiveModel = model.into();
                    active.title = Set(event.title);
                    active.start = Set(event.start);
                    active.end = Set(event.end);
                    active.color = Set(event.color);
                    active.update(&txn).await?;
                    updated += 1;
                }
            }
            None => {
                let active = ActiveModel {
                    uid: Set(event.uid),
                    title: Set(event.title),
                    start: Set(event.start),
                    end: Set(event.end),
                    color: Set(event.color),
                    ..Default::default()
                };
                active.insert(&txn).await?;
                created += 1;
            }
        }
    }

    let deleted = Events::delete_many()
        .filter(Column::Uid.is_not_in(seen_uids))
        .exec(&txn)
        .await?
        .rows_affected as usize;

    txn.commit().await?;

    Ok((created, updated, deleted))
}
