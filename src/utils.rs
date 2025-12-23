use chrono::{Datelike, Local};
use sea_orm::sqlx::types::chrono::{DateTime as ChronoDateTime, FixedOffset};

pub fn check_student_id(student_id: impl AsRef<str>) -> bool {
    let id = student_id.as_ref();
    let chars = id.chars().collect::<Vec<char>>();
    if chars.len() != 8 {
        return false;
    }
    let current_year = (Local::now().year() - 1911) as u8;
    let first_char = chars[0];
    let year = &chars[1..=2];
    let department = &chars[3..=4];
    let class = chars[5].to_digit(10);
    let number = &chars[6..=7];
    if first_char != '0' {
        return false;
    }
    match year.iter().collect::<String>().parse::<u8>() {
        Ok(year_parsed) => {
            if year_parsed > (current_year % 100) {
                return false;
            }
        }
        Err(_) => return false,
    }
    if let Err(_) = u8::from_str_radix(&department.iter().collect::<String>(), 16) {
        return false;
    }
    match class {
        Some(class) => {
            if class > 1 {
                return false;
            }
        }
        None => return false,
    }
    match u8::from_str_radix(&number.iter().collect::<String>(), 10) {
        Ok(number) => {
            if number > 99 || number < 1 {
                return false;
            }
        }
        Err(_) => return false,
    }
    return true;
}

pub fn classroom_key(id: &str) -> String {
    format!("classroom_{}", id)
}

pub fn classroom_with_keys_key(id: &str) -> String {
    format!("classroom_{}_keys", id)
}

pub fn classroom_with_reservations_key(id: &str) -> String {
    format!("classroom_{}_reservations", id)
}

pub fn classroom_with_keys_and_reservations_key(id: &str) -> String {
    format!("classroom_{}_keys_reservations", id)
}

// ===============================
//   datetime parser (minimal add)
// ===============================
pub fn parse_dt(s: &str) -> Result<ChronoDateTime<FixedOffset>, ()> {
    let raw = s.trim();

    // 1) already has offset / Z
    if let Ok(dt) = raw.parse::<ChronoDateTime<FixedOffset>>() {
        return Ok(dt);
    }

    // 2) normalize then append +08:00 (Taiwan)
    let mut base = raw.to_string();

    // "YYYY-MM-DD HH:MM" -> "YYYY-MM-DDTHH:MM"
    if base.as_bytes().get(10) == Some(&b' ') {
        base.replace_range(10..11, "T");
    }

    // add seconds
    if base.len() == 16 {
        base.push_str(":00");
    }

    // add timezone
    base.push_str("+08:00");

    base.parse::<ChronoDateTime<FixedOffset>>().map_err(|_| ())
}