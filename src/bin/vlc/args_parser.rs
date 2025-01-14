use std::ffi::OsString;
use std::fmt::{Display, Formatter};

#[derive(Clone)]
pub struct Interval {
    min: i64,
    max: i64,
}

impl Interval {
    pub fn contains(&self, value: i64) -> bool {
        value > self.min && value <= self.max
    }
}
impl Display for Interval {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}-{}", self.min, self.max)
    }
}

pub struct Intervals {
    parts: Vec<Interval>,
}

impl Intervals {
    fn new() -> Self {
        Intervals {
            parts: vec![]
        }
    }

    fn add(&mut self, min: i64, max: i64) {
        self.parts.push(Interval { min, max })
    }


    pub fn in_which_interval(&self, value: i64) -> Option<&Interval> {
        self.parts.iter().find(|&interval| interval.contains(value))
    }
}

impl From<OsString> for Intervals {
    fn from(value: OsString) -> Self {
        let mut intervals = Intervals::new();

        for part in value.to_str().unwrap().split(',') {
            let parts: Vec<&str> = part.split(':').collect();
            let min = parts[0].parse::<i64>().unwrap();
            let max = parts[1].parse::<i64>().unwrap();
            intervals.add(min, max);
        }
        intervals
    }
}

impl Clone for Intervals {
    fn clone(&self) -> Self {
        Intervals {
            parts: self.parts.clone()
        }
    }
}

