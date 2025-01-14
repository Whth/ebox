#[cfg(test)]
mod tests {
    use crate::AsDuration;
    use chrono::Duration;

    #[test]
    fn test_valid_time_format() {
        assert_eq!(
            "00:12:03".as_duration(),
            Some(Duration::seconds(723))
        );

        assert_eq!(
            "03:12".as_duration(),
            Some(Duration::seconds(192))
        );
    }

    #[test]
    fn test_invalid_time_format() {
        assert_eq!("invalid".as_duration(), None); // Invalid format
    }

    #[test]
    fn test_empty_string() {
        assert_eq!("".as_duration(), None);
    }

    #[test]
    fn test_single_digit_values() {
        assert_eq!("0:03".as_duration(), None);
        assert_eq!("00:03".as_duration(), Some(Duration::seconds(3)));
        assert_eq!("asvas00:03asvad".as_duration(), Some(Duration::seconds(3)));
        assert_eq!("00:00:03".as_duration(), Some(Duration::seconds(3)));
    }
}