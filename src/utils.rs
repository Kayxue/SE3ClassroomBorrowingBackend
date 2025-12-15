use chrono::{Datelike, Local};

pub fn check_student_id(student_id: impl AsRef<str>) -> bool {
    let id = student_id.as_ref();
    let chars = id.chars().collect::<Vec<char>>();
    if chars.len() != 8 {
        return false;
    }
    let current_year = Local::now().year() - 1911;
    let first_char = chars[0];
    let year = &chars[1..=2];
    let department = &chars[3..=4];
    let class = chars[5].to_digit(10);
    let number = &chars[6..=7];
    if first_char != '0' {
        return false;
    }
    if let Ok(year_parsed) = year.iter().collect::<String>().parse::<i32>() {
        if year_parsed < 0 || year_parsed > (current_year % 100) {
            return false;
        }
    } else {
        return false;
    }
    if let Err(_) = u8::from_str_radix(&department.iter().collect::<String>(), 16) {
        return false;
    }
    if class.is_none() || class.unwrap() > 1 {
        return false;
    }
    if u8::from_str_radix(&number.iter().collect::<String>(), 10).is_err() {
        return false;
    }
    return true;
}
