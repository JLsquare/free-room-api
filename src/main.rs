use chrono::{NaiveDateTime, Utc, Duration};
use std::collections::{HashMap, HashSet};
use thiserror::Error;
use actix_web::{web, App, HttpServer, HttpResponse, get, ResponseError};
use serde::Serialize;
use std::sync::Arc;
use actix_cors::Cors;
use ical::IcalParser;
use ical::parser::ical::component::IcalEvent;
use ical::parser::ParserError;
use regex::Regex;
use tokio::time;
use tokio::sync::Mutex;
use serde_json::Error as SerdeError;

const START_WEEK_OFFSET: i64 = 2;
const END_WEEK_OFFSET: i64 = 8;
const ICAL_DATE_FORMAT: &str = "%Y%m%dT%H%M%SZ";
const UBS_DATE_FORMAT: &str = "%Y-%m-%d";
const RESOURCES: [i32; 118] = [
    726, 1508, 730, 1649, 731, 1680, 706, 1698, 733, 1715,
    707, 5805, 3400, 3403, 3404, 7957, 7958, 4816, 7834, 7835,
    4501, 4722, 4624, 3395, 4727, 1037, 1981, 3584, 1884, 3586,
    1803, 3582, 1274, 3587, 3402, 1290, 2016, 3513, 3543, 3542,
    3538, 3535, 3532, 3530, 3527, 3525, 3487, 3486, 3484, 3483,
    3479, 3478, 4294, 4296, 6345, 6927, 6932, 6974, 5252, 4226,
    3508, 3510, 5877, 3577, 3997, 4209, 1849, 1359, 6791, 6800,
    1890, 6787, 6789, 2876, 3467, 3466, 3464, 3463, 3461, 3460,
    3458, 3457, 3453, 3454, 3450, 3451, 3447, 3448, 1299, 1189,
    3492, 3493, 3438, 3436, 3433, 3431, 3429, 3428, 3426, 3425,
    3387, 3585, 3580, 71, 2883, 2902, 2808, 2811, 2814, 2836,
    3421, 3420, 3412, 3411, 3415, 3414, 3418, 3417
];

#[derive(Error, Debug)]
enum AppError {
    #[error("network request failed")]
    Network(#[from] reqwest::Error),
    #[error("failed to parse date")]
    ChronoParse(#[from] chrono::format::ParseError),
    #[error("std io error")]
    Std(#[from] std::io::Error),
    #[error("regex error")]
    Regex(#[from] regex::Error),
    #[error("parser error")]
    ParserError,
    #[error("parse error")]
    ParseError,
    #[error("serde json error")]
    SerdeJson(#[from] SerdeError),
    #[error("ical parsing error")]
    IcalParse(#[from] ParserError),
}

impl ResponseError for AppError {
    fn error_response(&self) -> HttpResponse {
        match *self {
            AppError::Network(_) => HttpResponse::ServiceUnavailable().json("Network error"),
            AppError::ChronoParse(_) => HttpResponse::InternalServerError().json("Chrono parse error"),
            AppError::Std(_) => HttpResponse::InternalServerError().json("Std error"),
            AppError::Regex(_) => HttpResponse::BadRequest().json("Invalid input"),
            AppError::ParserError => HttpResponse::BadRequest().json("Parser error"),
            AppError::ParseError => HttpResponse::BadRequest().json("Parse error"),
            AppError::SerdeJson(_) => HttpResponse::InternalServerError().json("Serde json error"),
            AppError::IcalParse(_) => HttpResponse::InternalServerError().json("Ical parse error"),
        }
    }
}

#[derive(Serialize)]
struct Room {
    name: String,
    #[serde(skip_serializing)]
    slots: HashSet<(i64, i64)>,
    availability: Vec<(i64, i64)>,
}

impl Room {
    fn new(name: String) -> Self {
        Room {
            name,
            slots: HashSet::new(),
            availability: Vec::new(),
        }
    }

    fn compute_availability(&mut self, current_timestamp: i64) {
        let mut sorted_slots: Vec<_> = self.slots.iter().cloned().collect();
        sorted_slots.sort_by(|a, b| a.0.cmp(&b.0));

        self.availability.clear();
        let mut last_end = current_timestamp;
        for &(start, end) in &sorted_slots {
            if start > last_end {
                self.availability.push((last_end, start));
            }
            if end > last_end {
                last_end = end;
            }
        }

        self.availability.push((last_end, current_timestamp));
    }
}

#[derive(Serialize)]
struct RoomAvailability {
    name: String,
    status: String,
    duration: i64,
    open: bool,
}

#[tokio::main]
async fn main() -> Result<(), AppError> {
    let rooms = Arc::new(Mutex::new(HashMap::new()));
    let rooms_clone = rooms.clone();

    tokio::spawn(async move {
        let mut interval = time::interval(time::Duration::from_secs(3600));
        loop {
            interval.tick().await;
            update_rooms(&rooms_clone).await;
        }
    });

    HttpServer::new(move || {
        App::new()
            .wrap(
                Cors::permissive()
            )
            .app_data(web::Data::new(rooms.clone()))
            .service(get_all_rooms_info)
            .service(get_rooms_availability)
    })
        .bind("127.0.0.1:8080")?
        .run()
        .await?;

    Ok(())
}

async fn update_rooms(rooms: &Arc<Mutex<HashMap<String, Room>>>) {
    let start_date = Utc::now().naive_utc().date() - Duration::weeks(START_WEEK_OFFSET);
    let end_date = start_date + Duration::weeks(END_WEEK_OFFSET);

    for resource in RESOURCES.iter() {
        let mut rooms_guard = rooms.lock().await;
        if let Err(e) = process_resource(resource, &mut rooms_guard, &start_date, &end_date).await {
            eprintln!("Error processing resource {}: {}", resource, e);
        }
    }
}

#[get("/api/all")]
async fn get_all_rooms_info(
    data: web::Data<Arc<Mutex<HashMap<String, Room>>>>
) -> Result<HttpResponse, AppError> {
    let mut rooms = HashMap::new();
    let regex = Regex::new(r"^\bV-[AB]\s?\d*?\b$")?;
    for room in data.lock().await.values_mut() {
        if regex.is_match(&room.name) && !room.availability.is_empty() {
            room.compute_availability(Utc::now().naive_utc().timestamp());
            rooms.insert(room.name.clone(), room.availability.clone());
        }
    }
    let rooms_json = serde_json::to_string(&rooms)?;
    Ok(HttpResponse::Ok().content_type("application/json").body(rooms_json))
}

#[get("/api/lite/{hour_offset}")]
async fn get_rooms_availability(
    data: web::Data<Arc<Mutex<HashMap<String, Room>>>>,
    path: web::Path<i64>,
) -> Result<HttpResponse, AppError> {
    let mut rooms = data.lock().await;
    let offset = path.into_inner() * 3600;
    let current_timestamp = Utc::now().naive_utc().timestamp() + offset;
    let mut room_availabilities = Vec::new();
    let regex = Regex::new(r"^\bV-[AB]\s?\d*?\b$")?;

    for (name, room) in rooms.iter_mut() {
        if regex.is_match(name) {
            room.compute_availability(current_timestamp);
            let availability_info = calculate_room_availability(room, current_timestamp)?;
            room_availabilities.push(RoomAvailability {
                name: name.clone(),
                status: availability_info.0,
                duration: availability_info.1,
                open: availability_info.2,
            });
        }
    }

    room_availabilities.sort_by(|a, b| a.name.cmp(&b.name));
    let rooms_json = serde_json::to_string(&room_availabilities)?;
    Ok(HttpResponse::Ok().content_type("application/json").body(rooms_json))
}

fn calculate_room_availability(room: &Room, current_timestamp: i64) -> Result<(String, i64, bool), AppError> {
    let today_8am = Utc::now()
        .naive_utc()
        .date()
        .and_hms_opt(8, 0, 0)
        .ok_or_else(|| AppError::ParseError)?
        .timestamp();
    let tomorrow_8am = today_8am + 86400;
    let mut open = false;
    for &(start, end) in &room.availability {
        if start >= today_8am && end <= tomorrow_8am {
            open = true;
        }

        if start <= current_timestamp && current_timestamp < end {
            return Ok(("available".to_string(), end - current_timestamp, open));
        } else if start > current_timestamp {
            return Ok(("unavailable".to_string(), start - current_timestamp, open));
        }
    }
    Ok(("unavailable".to_string(), -1, false))
}

async fn process_resource(
    resource: &i32,
    rooms: &mut HashMap<String, Room>,
    start_date: &chrono::NaiveDate,
    end_date: &chrono::NaiveDate
) -> Result<(), AppError> {
    let url = format_resource_url(resource, start_date, end_date);
    let ics = reqwest::get(&url).await?.text().await?;
    let calendar = IcalParser::new(ics.as_bytes()).next().ok_or(AppError::ParserError)??;

    for event in calendar.events {
        process_event(event, rooms)?;
    }

    Ok(())
}

fn format_resource_url(
    resource: &i32,
    current_date: &chrono::NaiveDate,
    two_weeks_date: &chrono::NaiveDate
) -> String {
    format!("https://planning.univ-ubs.fr/jsp/custom/modules/plannings/anonymous_cal.jsp?resources={}&projectId=1&calType=ical&firstDate={}&lastDate={}",
            resource, current_date.format(UBS_DATE_FORMAT), two_weeks_date.format(UBS_DATE_FORMAT))
}

fn process_event(
    event: IcalEvent,
    rooms: &mut HashMap<String, Room>
) -> Result<(), AppError> {
    let property_value = event.properties[4].value.clone().unwrap_or_default();
    let rooms_names = property_value.split("\\,").collect::<Vec<&str>>();

    for room_name in rooms_names {
        let room = rooms.entry(room_name.to_string()).or_insert_with(|| Room::new(room_name.to_string()));
        let start = NaiveDateTime::parse_from_str(&event.properties[1].value.clone().ok_or(AppError::ParseError)?, ICAL_DATE_FORMAT)?.timestamp();
        let end = NaiveDateTime::parse_from_str(&event.properties[2].value.clone().ok_or(AppError::ParseError)?, ICAL_DATE_FORMAT)?.timestamp();
        room.slots.insert((start, end));
    }

    Ok(())
}