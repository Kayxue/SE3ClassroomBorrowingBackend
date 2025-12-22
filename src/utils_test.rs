#[cfg(test)]
mod tests {
    use super::super::utils::check_student_id;
    use chrono::{Datelike, Local};

    #[test]
    fn test_valid_student_id() {
        // Test with a valid student ID using current year
        let current_year = Local::now().year() - 1911;
        let valid_year = format!("{:02}", current_year % 100);
        let valid_id = format!("0{}1E001", valid_year);
        assert!(check_student_id(&valid_id));
    }

    #[test]
    fn test_valid_student_id_old_year() {
        // Test with an older valid year
        let current_year = Local::now().year() - 1911;
        let valid_year = format!("{:02}", (current_year % 100).saturating_sub(5));
        let valid_id = format!("0{}2A101", valid_year);
        assert!(check_student_id(&valid_id));
    }

    #[test]
    fn test_invalid_length_too_short() {
        assert!(!check_student_id("0121E01"));
    }

    #[test]
    fn test_invalid_length_too_long() {
        assert!(!check_student_id("0121E0012"));
    }

    #[test]
    fn test_empty_string() {
        assert!(!check_student_id(""));
    }

    #[test]
    fn test_invalid_first_character_numeric() {
        assert!(!check_student_id("1121E001"));
    }

    #[test]
    fn test_invalid_first_character_letter() {
        assert!(!check_student_id("A121E001"));
    }

    #[test]
    fn test_invalid_year_future() {
        // Test with a future year (greater than current year)
        let current_year = Local::now().year() - 1911;
        let future_year = format!("{:02}", (current_year % 100) + 1);
        let invalid_id = format!("0{}1E001", future_year);
        assert!(!check_student_id(&invalid_id));
    }

    #[test]
    fn test_invalid_year_non_numeric() {
        assert!(!check_student_id("0AB1E001"));
    }

    #[test]
    fn test_invalid_year_special_chars() {
        assert!(!check_student_id("0-11E001"));
    }

    #[test]
    fn test_valid_department_hex_lowercase() {
        let current_year = Local::now().year() - 1911;
        let valid_year = format!("{:02}", current_year % 100);

        // Test various hex department codes (lowercase)
        let departments = vec!["1a", "2b", "3c", "4d", "5e", "6f", "ab", "cd", "ef"];
        for dept in departments {
            let valid_id = format!("0{}{}001", valid_year, dept);
            assert!(
                check_student_id(&valid_id),
                "Failed for department: {}",
                dept
            );
        }
    }

    #[test]
    fn test_valid_department_hex_uppercase() {
        let current_year = Local::now().year() - 1911;
        let valid_year = format!("{:02}", current_year % 100);

        // Test various hex department codes (uppercase)
        let departments = vec!["1A", "2B", "3C", "4D", "5E", "6F", "AB", "CD", "EF"];
        for dept in departments {
            let valid_id = format!("0{}{}001", valid_year, dept);
            assert!(
                check_student_id(&valid_id),
                "Failed for department: {}",
                dept
            );
        }
    }

    #[test]
    fn test_valid_department_numeric() {
        let current_year = Local::now().year() - 1911;
        let valid_year = format!("{:02}", current_year % 100);

        // Test numeric department codes
        let departments = vec![
            "00", "01", "12", "23", "34", "45", "56", "67", "78", "89", "99",
        ];
        for dept in departments {
            let valid_id = format!("0{}{}001", valid_year, dept);
            assert!(
                check_student_id(&valid_id),
                "Failed for department: {}",
                dept
            );
        }
    }

    #[test]
    fn test_invalid_department_non_hex() {
        let current_year = Local::now().year() - 1911;
        let valid_year = format!("{:02}", current_year % 100);

        // G is not a valid hex character
        let invalid_id = format!("0{}1G001", valid_year);
        assert!(!check_student_id(&invalid_id));
    }

    #[test]
    fn test_invalid_department_special_chars() {
        let current_year = Local::now().year() - 1911;
        let valid_year = format!("{:02}", current_year % 100);

        assert!(!check_student_id(&format!("0{}1-001", valid_year)));
        assert!(!check_student_id(&format!("0{}1_001", valid_year)));
        assert!(!check_student_id(&format!("0{}1@001", valid_year)));
    }

    #[test]
    fn test_valid_class_zero() {
        let current_year = Local::now().year() - 1911;
        let valid_year = format!("{:02}", current_year % 100);
        let id_class_0 = format!("0{}1E001", valid_year);
        assert!(check_student_id(&id_class_0));
    }

    #[test]
    fn test_valid_class_one() {
        let current_year = Local::now().year() - 1911;
        let valid_year = format!("{:02}", current_year % 100);
        let id_class_1 = format!("0{}1E101", valid_year);
        assert!(check_student_id(&id_class_1));
    }

    #[test]
    fn test_invalid_class_two() {
        let current_year = Local::now().year() - 1911;
        let valid_year = format!("{:02}", current_year % 100);
        let invalid_id = format!("0{}1E201", valid_year);
        assert!(!check_student_id(&invalid_id));
    }

    #[test]
    fn test_invalid_class_nine() {
        let current_year = Local::now().year() - 1911;
        let valid_year = format!("{:02}", current_year % 100);
        let invalid_id = format!("0{}1E901", valid_year);
        assert!(!check_student_id(&invalid_id));
    }

    #[test]
    fn test_invalid_class_letter() {
        let current_year = Local::now().year() - 1911;
        let valid_year = format!("{:02}", current_year % 100);
        let invalid_id = format!("0{}1EA01", valid_year);
        assert!(!check_student_id(&invalid_id));
    }

    #[test]
    fn test_valid_number_range() {
        let current_year = Local::now().year() - 1911;
        let valid_year = format!("{:02}", current_year % 100);

        // Test various student numbers (01-99)
        let numbers = vec!["01", "10", "25", "50", "75", "99"];
        for num in numbers {
            let valid_id = format!("0{}1E0{}", valid_year, num);
            assert!(check_student_id(&valid_id), "Failed for number: {}", num);
        }
    }

    #[test]
    fn test_invalid_number_non_numeric() {
        let current_year = Local::now().year() - 1911;
        let valid_year = format!("{:02}", current_year % 100);

        assert!(!check_student_id(&format!("0{}1E0AB", valid_year)));
        assert!(!check_student_id(&format!("0{}1E0XY", valid_year)));
    }

    #[test]
    fn test_invalid_number_special_chars() {
        let current_year = Local::now().year() - 1911;
        let valid_year = format!("{:02}", current_year % 100);

        assert!(!check_student_id(&format!("0{}1E0-1", valid_year)));
        assert!(!check_student_id(&format!("0{}1E0_1", valid_year)));
    }

    #[test]
    fn test_invalid_number_zero() {
        let current_year = Local::now().year() - 1911;
        let valid_year = format!("{:02}", current_year % 100);

        // Number must be between 1 and 99, so "00" is invalid
        assert!(!check_student_id(&format!("0{}1E000", valid_year)));
    }

    #[test]
    fn test_boundary_year_exact_current() {
        // Test with exactly the current year (should be valid)
        let current_year = Local::now().year() - 1911;
        let boundary_year = format!("{:02}", current_year % 100);
        let boundary_id = format!("0{}1E001", boundary_year);
        assert!(check_student_id(&boundary_id));
    }

    #[test]
    fn test_year_00() {
        // Year 00 should be valid
        assert!(check_student_id("0001E001"));
    }

    #[test]
    fn test_complete_valid_examples() {
        let current_year = Local::now().year() - 1911;
        let valid_year = format!("{:02}", current_year % 100);

        // Test complete valid student IDs
        let valid_ids = vec![
            format!("0{}00001", valid_year),
            format!("0{}AB099", valid_year),
            format!("0{}FF150", valid_year),
            format!("0{}12001", valid_year),
        ];

        for id in valid_ids {
            assert!(check_student_id(&id), "Failed for ID: {}", id);
        }
    }

    #[test]
    fn test_unicode_characters() {
        assert!(!check_student_id("0121E😀01"));
        assert!(!check_student_id("012😀E001"));
    }

    #[test]
    fn test_whitespace() {
        assert!(!check_student_id("0121E0 1"));
        assert!(!check_student_id(" 121E001"));
        assert!(!check_student_id("0121E001 "));
    }

    #[test]
    fn test_case_sensitivity_of_hex() {
        let current_year = Local::now().year() - 1911;
        let valid_year = format!("{:02}", current_year % 100);

        // Both lowercase and uppercase hex should be valid
        assert!(check_student_id(&format!("0{}ab001", valid_year)));
        assert!(check_student_id(&format!("0{}AB001", valid_year)));
        assert!(check_student_id(&format!("0{}Ab001", valid_year)));
        assert!(check_student_id(&format!("0{}aB001", valid_year)));
    }
}
