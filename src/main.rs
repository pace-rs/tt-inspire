use anyhow::Result;
use chrono::{prelude::*, serde::ts_seconds, Duration, NaiveDate, NaiveDateTime, NaiveTime};
use iif::iif;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use structopt::StructOpt;

mod settings;

use settings::Settings;

#[derive(Debug, StructOpt)]
struct Options {
    #[cfg(feature = "binary")]
    /// which data file to use. [default: ~/timetracking.bin]
    #[structopt(short, long)]
    data_file: Option<PathBuf>,

    #[cfg(not(feature = "binary"))]
    /// which data file to use. [default: ~/timetracking.json]
    #[structopt(short, long)]
    data_file: Option<PathBuf>,

    #[structopt(subcommand)]
    command: Option<Command>,
}

#[derive(Default, Debug, StructOpt)]
struct FilterData {
    /// show all entries after this point in time [defaults to current day 00:00:00]
    /// allowed formats are: "%Y-%m-%d %H:%M:%S", "%Y-%m-%d", "%H:%M:%S"
    #[structopt(short, long)]
    from: Option<String>,

    /// show all entries before this point in time [defaults to start day 23:59:59]
    /// allowed formats are: "%Y-%m-%d %H:%M:%S", "%Y-%m-%d", "%H:%M:%S"
    #[structopt(short, long)]
    to: Option<String>,

    /// filter entries. possible filter values: "week", "all" or part of the description
    filter: Option<String>,
}

#[derive(Debug, StructOpt)]
enum Command {
    // keep this at the top, otherwise rust analyzer will underline the whole struct until this
    // point as it thinks there is a problem, because it doesn't understand that this variant is
    // disabled via attribute.
    #[cfg(not(feature = "binary"))]
    /// export data to file
    Export {
        /// where to write the output file
        path: PathBuf,
    },

    /// show info from the latest entry. Returns the exit code 0, if the time tracking is currently
    /// active and -1 if not.
    Status,

    /// start time tracking
    Start {
        /// a description for the event
        description: Option<String>,

        /// the time at which the event happend.
        /// format: "HH:MM:SS" or "YY-mm-dd HH:MM:SS" [defaults to current time]
        #[structopt(short, long)]
        at: Option<String>,
    },

    /// stop time tracking
    Stop {
        /// a description for the event
        description: Option<String>,

        /// the time at which the event happend.
        /// format: "HH:MM:SS" or "YY-mm-dd HH:MM:SS" [defaults to current time]
        #[structopt(short, long)]
        at: Option<String>,
    },

    /// continue time tracking with last description
    Continue,

    /// list all entries
    List {
        #[structopt(flatten)]
        filter: FilterData,
    },

    /// show path to data file
    Path,

    /// show work time for given timespan
    Show {
        #[structopt(flatten)]
        filter: FilterData,

        /// show only the time with no additional text
        #[structopt(short, long)]
        plain: bool,

        /// show time until the defined time goals are met.
        #[structopt(short, long)]
        remaining: bool,

        /// include seconds in time calculation
        #[structopt(short)]
        include_seconds: bool,

        /// show only the time with no additional text. [default: "{hh}:{mm}:{ss}"]
        #[structopt(long)]
        format: Option<String>,
    },
    #[cfg(feature = "binary")]
    /// export data to file
    Export {
        /// export in a human readable format. This format is for human reading only and cannot be
        /// imported
        #[structopt(short, long)]
        readable: bool,
        /// pretty print json
        #[structopt(short, long)]
        pretty: bool,
        /// where to write the output file
        path: PathBuf,
    },
    #[cfg(feature = "binary")]
    /// import data from json file
    Import {
        /// which file to import
        path: PathBuf,
    },
}

impl Default for Command {
    fn default() -> Self {
        Self::Show {
            filter: FilterData::default(),
            format: None,
            include_seconds: false,
            plain: false,
            remaining: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TrackingData {
    description: Option<String>,

    #[serde(with = "ts_seconds")]
    time: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
enum TrackingEvent {
    Start(TrackingData),
    Stop(TrackingData),
}

impl TrackingEvent {
    fn time(&self, include_seconds: bool) -> DateTime<Utc> {
        match self {
            Self::Start(TrackingData { time, .. }) | Self::Stop(TrackingData { time, .. }) => {
                let time = *time;
                if include_seconds {
                    time
                } else {
                    time.with_second(0).expect("could not set seconds to zero")
                }
            }
        }
    }

    fn description(&self) -> Option<String> {
        match self {
            Self::Start(TrackingData { description, .. })
            | Self::Stop(TrackingData { description, .. }) => description.clone(),
        }
    }

    fn is_start(&self) -> bool {
        match self {
            Self::Start(_) => true,
            Self::Stop(_) => false,
        }
    }

    fn is_stop(&self) -> bool {
        match self {
            Self::Start(_) => false,
            Self::Stop(_) => true,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum DateOrDateTime {
    Date(NaiveDate),
    DateTime(NaiveDateTime),
}

#[cfg(feature = "binary")]
fn read_data<P: AsRef<Path>>(path: P) -> Result<Vec<TrackingEvent>> {
    let data = std::fs::read(&path)?;
    Ok(bincode::deserialize(&data)?)
}

#[cfg(not(feature = "binary"))]
fn read_data<P: AsRef<Path>>(path: P) -> Result<Vec<TrackingEvent>> {
    read_json_data(path)
}

fn read_json_data<P: AsRef<Path>>(path: P) -> Result<Vec<TrackingEvent>> {
    let data = std::fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&data)?)
}

#[cfg(feature = "binary")]
fn write_data<P: AsRef<Path>>(path: P, data: &[TrackingEvent]) {
    let data = bincode::serialize(data).expect("could not serialize data");
    std::fs::write(path, data).expect("could not write data file");
}

fn write_json_data<P: AsRef<Path>>(path: P, data: &[TrackingEvent], pretty: bool) {
    let data = iif!(
        pretty,
        serde_json::to_string_pretty(data),
        serde_json::to_string(data)
    )
    .expect("could not serialize data");
    std::fs::write(path, data).expect("could not write data file");
}

#[cfg(not(feature = "binary"))]
fn write_data<P: AsRef<Path>>(path: P, data: &[TrackingEvent]) {
    write_json_data(path, data, false);
}

fn start_tracking(
    settings: &Settings,
    data: &mut Vec<TrackingEvent>,
    description: Option<String>,
    at: Option<String>,
) {
    let (should_add, last_description) = match data.last() {
        None => (true, None),
        Some(event) => (event.is_stop(), event.description()),
    };
    if should_add {
        data.push(TrackingEvent::Start(TrackingData {
            description,
            time: at.map_or_else(|| Local::now().into(), |at| parse_date_time(&at)),
        }));
    } else if settings.auto_insert_stop && at.is_none() {
        match (description, last_description) {
            (Some(description), Some(last_description)) if description == last_description => {
                eprintln!(
                    "Timetracking with the description \"{}\" is already running!",
                    description
                )
            }
            (description, _) => {
                data.push(TrackingEvent::Stop(TrackingData {
                    description: None,
                    time: Local::now().into(),
                }));
                data.push(TrackingEvent::Start(TrackingData {
                    description,
                    time: Local::now().into(),
                }));
            }
        }
    } else if settings.auto_insert_stop && at.is_some() {
        eprintln!("Auto insert for stop events currently not supported with --at");
    } else {
        eprintln!("Time tracking is already running!");
    }
}

fn stop_tracking(data: &mut Vec<TrackingEvent>, description: Option<String>, at: Option<String>) {
    let should_add = match data.last() {
        None => true,
        Some(event) => event.is_start(),
    };
    if should_add {
        data.push(TrackingEvent::Stop(TrackingData {
            description,
            time: at.map_or_else(|| Local::now().into(), |at| parse_date_time(&at)),
        }))
    } else {
        eprintln!("Time tracking is already stopped!");
    }
}

fn continue_tracking(data: &mut Vec<TrackingEvent>) {
    if let Some(TrackingEvent::Stop { .. }) = data.last() {
        if let Some(TrackingEvent::Start(TrackingData { description, .. })) =
            data.iter().rev().find(|t| t.is_start()).cloned()
        {
            data.push(TrackingEvent::Start(TrackingData {
                description,
                time: Local::now().into(),
            }))
        }
    } else {
        eprintln!("Time tracking couldn't be continued, because there are no entries. Use the start command instead!");
    }
}

fn split_duration(duration: Duration) -> (i64, i64, i64) {
    let hours = duration.num_hours();
    let hours_in_minutes = hours * 60;
    let hours_in_seconds = hours_in_minutes * 60;
    let minutes = duration.num_minutes() - hours_in_minutes;
    let minutes_in_seconds = minutes * 60;
    let seconds = duration.num_seconds() - hours_in_seconds - minutes_in_seconds;
    (hours, minutes, seconds)
}

fn filter_events(
    data: &[TrackingEvent],
    from: &Option<String>,
    to: &Option<String>,
    filter: &Option<String>,
) -> Vec<TrackingEvent> {
    let (filter, from, to) = match filter {
        Some(from) if from == "week" => {
            let now = Local::today();
            let weekday = now.weekday();
            let offset = weekday.num_days_from_monday();
            let (monday_offset, sunday_offset) = (offset, 6 - offset);
            let from = DateOrDateTime::Date(
                now.with_day(now.day() - monday_offset)
                    .unwrap()
                    .naive_local(),
            );
            let to = DateOrDateTime::Date(
                now.with_day(now.day() + sunday_offset)
                    .unwrap()
                    .naive_local(),
            );
            (None, Some(from), Some(to))
        }
        f => {
            let from = from.as_ref().map_or_else(
                || DateOrDateTime::Date(Local::today().naive_local()),
                |s| parse_date_or_date_time(&s),
            );

            let to = match to {
                Some(s) => parse_date_or_date_time(&s),
                None => match from {
                    DateOrDateTime::DateTime(from) => DateOrDateTime::Date(from.date()),
                    from @ DateOrDateTime::Date(..) => from,
                },
            };
            (f.clone(), Some(from), Some(to))
        }
    };
    let data_iterator = data
        .iter()
        .filter(|entry| {
            iif!(
                filter.clone().unwrap_or_default() == "all",
                true,
                match from {
                    None => true,
                    Some(DateOrDateTime::Date(from)) => {
                        entry.time(true).timestamp_millis()
                            >= TimeZone::from_local_date(&Local, &from)
                                .unwrap()
                                .and_time(NaiveTime::from_hms(0, 0, 0))
                                .unwrap()
                                .timestamp_millis()
                    }
                    Some(DateOrDateTime::DateTime(from)) => {
                        entry.time(true).timestamp_millis()
                            >= TimeZone::from_local_datetime(&Local, &from)
                                .unwrap()
                                .timestamp_millis()
                    }
                }
            )
        })
        .filter(|entry| {
            iif!(
                filter.clone().unwrap_or_default() == "all",
                true,
                match to {
                    None => true,
                    Some(DateOrDateTime::Date(to)) => {
                        entry.time(true).timestamp_millis()
                            <= TimeZone::from_local_date(&Local, &to)
                                .unwrap()
                                .and_time(NaiveTime::from_hms(23, 59, 59))
                                .unwrap()
                                .timestamp_millis()
                    }
                    Some(DateOrDateTime::DateTime(to)) => {
                        entry.time(true).timestamp_millis()
                            <= TimeZone::from_local_datetime(&Local, &to)
                                .unwrap()
                                .timestamp_millis()
                    }
                }
            )
        })
        .filter(|entry| match entry {
            TrackingEvent::Start(TrackingData { description, .. })
            | TrackingEvent::Stop(TrackingData { description, .. }) => match (&filter, description)
            {
                (Some(filter), Some(description)) => {
                    filter == "all" || description.contains(filter)
                }
                (Some(filter), None) => filter == "all",
                (None, _) => true,
            },
        })
        .skip_while(|entry| TrackingEvent::is_stop(entry));
    data_iterator.cloned().collect()
}

fn get_time_from_events(data: &[TrackingEvent], include_seconds: bool) -> Duration {
    let mut data_iterator = data.iter();
    let mut work_day = Duration::zero();
    loop {
        let start = data_iterator.next();
        let stop = data_iterator.next();
        match (start, stop) {
            (Some(start), Some(stop)) => {
                let duration = stop.time(include_seconds) - start.time(include_seconds);
                work_day = work_day
                    .checked_add(&duration)
                    .expect("couldn't add up durations");
            }
            (Some(start), None) => {
                let now = if include_seconds {
                    Utc::now()
                } else {
                    Utc::now().with_second(0).unwrap()
                };
                let duration = now - start.time(include_seconds);
                work_day = work_day
                    .checked_add(&duration)
                    .expect("couldn't add up durations");
                break;
            }
            (_, _) => break,
        }
    }
    work_day
}

fn get_remaining_minutes(settings: &Settings, filter: &str, hours: i64, minutes: i64) -> i64 {
    let total = minutes + (hours * 60);
    let time_goal = if filter == "week" {
        &settings.time_goal.weekly
    } else {
        &settings.time_goal.daily
    };
    let required = i64::from(time_goal.minutes) + (i64::from(time_goal.hours) * 60);
    required - total
}

fn show(
    settings: &Settings,
    data: &[TrackingEvent],
    filter: &FilterData,
    format: Option<String>,
    include_seconds: bool,
    plain: bool,
    remaining: bool,
) {
    let FilterData { from, to, filter } = filter;
    let data = filter_events(data, &from, &to, &filter);
    let work_time = get_time_from_events(&data, include_seconds);
    let (mut hours, mut minutes, mut seconds) = split_duration(work_time);

    let filter = filter.clone().unwrap_or_default();
    if remaining {
        if (filter == "week" || filter.is_empty()) && from.is_none() && to.is_none() {
            seconds = 0;
            let mut remaining_minutes = get_remaining_minutes(&settings, &filter, hours, minutes);

            if filter != "week" {
                let data = filter_events(&data, &None, &None, &Some("week".to_string()));
                let work_time = get_time_from_events(&data, include_seconds);
                let (week_hours, week_minutes, _) = split_duration(work_time);
                let remaining_minutes_week =
                    get_remaining_minutes(&settings, "week", week_hours, week_minutes);
                remaining_minutes = remaining_minutes.min(remaining_minutes_week);
            }

            hours = remaining_minutes / 60;
            minutes = remaining_minutes - (hours * 60);
        } else {
            eprintln!("Remaining only works when \"from\" and \"to\" are not set and with no filter or filter \"week\"");
            return;
        }
    }
    let format = format.unwrap_or_else(|| "{hh}:{mm}:{ss}".to_string());
    let time = format
        .replace("{hh}", &format!("{:02}", hours))
        .replace("{mm}", &format!("{:02}", minutes))
        .replace("{ss}", &format!("{:02}", seconds))
        .replace("{h}", &format!("{}", hours))
        .replace("{m}", &format!("{}", minutes))
        .replace("{s}", &format!("{}", seconds));
    if plain {
        println!("{}", time);
    } else if remaining {
        println!("Remaining Work Time: {}", time);
    } else {
        println!("Work Time: {}", time);
    }
}

fn status(data: &[TrackingEvent]) {
    if let Some(event) = data.last() {
        let time = event.time(true).with_timezone(&Local);
        let active = event.is_start();
        let text = iif!(active, "Start", "End");
        if let Some(description) = event.description() {
            println!("Active: {}", active);
            println!("Description: {}", description,);
            println!(
                "{} Time: {:02}:{:02}:{:02}",
                text,
                time.hour(),
                time.minute(),
                time.second()
            );
        } else {
            println!("Active: {}", active);
            println!(
                "{} Time: {:02}:{:02}:{:02}",
                text,
                time.hour(),
                time.minute(),
                time.second()
            );
        }
        std::process::exit(iif!(active, 0, -1));
    } else {
        println!("No Events found!");
        std::process::exit(-1);
    }
}

fn to_human_readable(prefix: &str, time: &DateTime<Utc>, description: Option<String>) -> String {
    let description = description
        .map(|d| format!(" \"{}\"", d))
        .unwrap_or_default();
    format!(
        "{}{} at {:04}.{:02}.{:02}-{:02}:{:02}:{:02}",
        prefix,
        description,
        time.year(),
        time.month(),
        time.day(),
        time.hour(),
        time.minute(),
        time.second()
    )
}

fn get_human_readable(data: &[TrackingEvent]) -> Vec<String> {
    data.iter()
        .map(|event| match event {
            TrackingEvent::Start(TrackingData { time, description }) => {
                to_human_readable("Start", time, description.clone())
            }
            TrackingEvent::Stop(TrackingData { time, description }) => {
                to_human_readable("Stop", time, description.clone())
            }
        })
        .collect::<Vec<_>>()
}

fn export_human_readable(path: String, data: &[TrackingEvent]) {
    let lines = get_human_readable(data);
    std::fs::write(path, lines.join("\n")).expect("could not export file");
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Options { command, data_file } = Options::from_args();

    let settings = Settings::new()?;

    let path = match data_file {
        Some(path) => path,
        None => shellexpand::full(&settings.data_file)?.parse()?,
    };
    let expanded_path = shellexpand::full(&path.to_string_lossy())
        .expect("could not expand path")
        .to_string();
    let mut data = read_data(&expanded_path).unwrap_or_default();

    let data_changed = match command.unwrap_or_default() {
        Command::Start { description, at } => {
            start_tracking(&settings, &mut data, description, at);
            true
        }
        Command::Stop { description, at } => {
            stop_tracking(&mut data, description, at);
            true
        }
        Command::Continue => {
            continue_tracking(&mut data);
            true
        }
        Command::List { filter } => {
            let data = filter_events(&data, &filter.from, &filter.to, &filter.filter);
            for s in get_human_readable(&data) {
                println!("{}", s);
            }
            false
        }
        Command::Path => {
            println!("{}", expanded_path);
            false
        }
        Command::Show {
            format,
            filter,
            include_seconds,
            plain,
            remaining,
        } => {
            show(
                &settings,
                &data,
                &filter,
                format,
                include_seconds,
                plain,
                remaining,
            );
            false
        }
        Command::Status => {
            status(&data);
            false
        }
        #[cfg(not(feature = "binary"))]
        Command::Export { path } => {
            let expanded_path = shellexpand::full(&path.to_string_lossy())
                .expect("could not expand path")
                .to_string();
            export_human_readable(expanded_path, &data);
            false
        }

        #[cfg(feature = "binary")]
        Command::Export {
            path,
            readable,
            pretty,
        } => {
            let expanded_path = shellexpand::full(&path.to_string_lossy())
                .expect("could not expand path")
                .to_string();
            if readable {
                export_human_readable(expanded_path, &data);
            } else {
                write_json_data(expanded_path, &data, pretty);
            }
            false
        }
        #[cfg(feature = "binary")]
        Command::Import { path } => {
            data = read_json_data(path)?;
            true
        }
        #[allow(unreachable_patterns)]
        _ => unimplemented!(),
    };

    if data_changed {
        write_data(expanded_path, &data);
    }

    Ok(())
}

fn parse_date_time(s: &str) -> DateTime<Utc> {
    if let Ok(time) = NaiveTime::parse_from_str(s, "%H:%M:%S") {
        let today = Local::today();
        let date_time = today.and_time(time).unwrap();
        return date_time.with_timezone(&Utc);
    }
    if let Ok(time) = NaiveTime::parse_from_str(&format!("{}:0", s), "%H:%M:%S") {
        let today = Local::today();
        let date_time = today.and_time(time).unwrap();
        return date_time.with_timezone(&Utc);
    }
    if let Ok(time) = NaiveTime::parse_from_str(&format!("{}:0:0", s), "%H:%M:%S") {
        let today = Local::today();
        let date_time = today.and_time(time).unwrap();
        return date_time.with_timezone(&Utc);
    }
    if let Ok(date_time) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return TimeZone::from_local_datetime(&Local, &date_time)
            .unwrap()
            .with_timezone(&Utc);
    }
    if let Ok(date_time) = NaiveDateTime::parse_from_str(&format!("{}:0", s), "%Y-%m-%d %H:%M:%S") {
        return TimeZone::from_local_datetime(&Local, &date_time)
            .unwrap()
            .with_timezone(&Utc);
    }
    let date_time =
        NaiveDateTime::parse_from_str(&format!("{}:0:0", s), "%Y-%m-%d %H:%M:%S").unwrap();
    TimeZone::from_local_datetime(&Local, &date_time)
        .unwrap()
        .with_timezone(&Utc)
}

fn parse_date_or_date_time(s: &str) -> DateOrDateTime {
    if let Ok(date) = NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
        return DateOrDateTime::Date(date);
    }
    if let Ok(date) =
        NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S").map(DateOrDateTime::DateTime)
    {
        return date;
    }
    if let Ok(date) = NaiveTime::parse_from_str(&s, "%H:%M:%S")
        .map(|time| Local::today().and_time(time).unwrap())
        .map(|date_time| date_time.naive_local())
        .map(DateOrDateTime::DateTime)
    {
        return date;
    }
    if let Ok(date) = NaiveTime::parse_from_str(&format!("{}:0", s), "%H:%M:%S")
        .map(|time| Local::today().and_time(time).unwrap())
        .map(|date_time| date_time.naive_local())
        .map(DateOrDateTime::DateTime)
    {
        return date;
    }
    if let Ok(date) = NaiveTime::parse_from_str(&format!("{}:0:0", s), "%H:%M:%S")
        .map(|time| Local::today().and_time(time).unwrap())
        .map(|date_time| date_time.naive_local())
        .map(DateOrDateTime::DateTime)
    {
        return date;
    }
    if let Ok(date) = NaiveDateTime::parse_from_str(&format!("{}:0", s), "%Y-%m-%d %H:%M:%S")
        .map(DateOrDateTime::DateTime)
    {
        return date;
    }
    NaiveDateTime::parse_from_str(&format!("{}:0:0", s), "%Y-%m-%d %H:%M:%S")
        .map(DateOrDateTime::DateTime)
        .unwrap()
}
