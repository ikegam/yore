use std::collections::BTreeMap;

use coordinates;

enum LocationMatch<'a> {
    Exact(&'a Location),
    First(&'a Location),
    Last(&'a Location),
    Between(&'a Location, &'a Location),
}

#[derive(Deserialize, PartialEq, Debug, Default)]
pub struct GoogleLocationHistory {
    #[serde(deserialize_with = "locations_sequence::deserialize")]
    locations: BTreeMap<i64, Location>,
}

impl GoogleLocationHistory {
    pub fn get_most_likely_location(&self, timestamp: i64) -> Option<&Location> {
        match self.location_at_time(timestamp) {
            None => None,
            Some(LocationMatch::Exact(location)) => Some(location),
            Some(LocationMatch::First(location)) => Some(location),
            Some(LocationMatch::Last(location)) => Some(location),
            Some(LocationMatch::Between(before, after)) => {
                if timestamp - before.timestamp_ms > after.timestamp_ms - timestamp {
                    Some(after)
                } else {
                    Some(before)
                }
            }
        }
    }

    /// If the given timestamp sits between two location timestamps, linearly interpolate
    /// between their two latitudes and longitudes. This is inaccurate at large gradients, as it
    /// doesn't take into account the curvature of the Earth, but in such cases interpolation
    /// is probably meaningless anyway as the points are probably not part of the same journey.
    pub fn interpolate_location(&self, timestamp: i64) -> Option<Location> {
        match self.location_at_time(timestamp) {
            None => None,
            Some(LocationMatch::Exact(location)) => Some(location.clone()),
            Some(LocationMatch::First(location)) => Some(location.clone()),
            Some(LocationMatch::Last(location)) => Some(location.clone()),
            Some(LocationMatch::Between(before, after)) => {
                let latitude_difference = after.latitude_e7 - before.latitude_e7;
                let longitude_difference = after.longitude_e7 - before.longitude_e7;
                let time_difference = after.timestamp_ms - before.timestamp_ms;

                let timestamp_ms = timestamp * 1000;
                let time_offset = timestamp_ms - before.timestamp_ms;

                let latitude_e7 = before.latitude_e7 +
                    latitude_difference * time_offset / time_difference;
                let longitude_e7 = before.longitude_e7 +
                    longitude_difference * time_offset / time_difference;
                let accuracy = interpolate_accuracy(timestamp_ms, before, after);

                Some(Location {
                    timestamp_ms,
                    latitude_e7,
                    longitude_e7,
                    accuracy,
                    activitys: None,
                })
            }
        }
    }

    fn location_at_time<'a>(&'a self, timestamp: i64) -> Option<LocationMatch<'a>> {
        let timestamp_ms = timestamp * 1000;

        if let Some(location) = self.locations.get(&timestamp_ms) {
            return Some(LocationMatch::Exact(location));
        }

        let before = self.locations.range(..timestamp_ms).last();
        let after = self.locations.range(timestamp_ms..).next();

        match (before, after) {
            (None, None) => None,
            (None, Some(after)) => Some(LocationMatch::First(after.1)),
            (Some(before), None) => Some(LocationMatch::Last(before.1)),
            (Some(before), Some(after)) => Some(LocationMatch::Between(before.1, after.1)),
        }
    }
}

/// Linearly scale between the accuracy of the nearest location data point and half the distance
/// between interpolated locations, according to the time difference between the given timestamp
/// the nearest location timestamp. If the half-distance is smaller than both location accuracies,
/// ignore it and linearly scale between the two accuracies instead.
fn interpolate_accuracy(timestamp_ms: i64, before: &Location, after: &Location) -> u16 {
    let time_offset = timestamp_ms - before.timestamp_ms;
    let time_difference = after.timestamp_ms - before.timestamp_ms;

    let half_distance = before.coordinates().distance_in_km(after.coordinates()) as i64 * 1000 / 2;
    let before_accuracy = before.accuracy as i64;
    let after_accuracy = after.accuracy as i64;

    let accuracy = if half_distance < before_accuracy && half_distance < after_accuracy {
        before_accuracy + (after_accuracy - before_accuracy) * time_offset / time_difference
    } else if time_offset <= time_difference / 2 {
        before_accuracy + (half_distance - before_accuracy) * time_offset * 2 / time_difference
    } else {
        half_distance +
            (after_accuracy - half_distance) * (time_offset * 2 - time_difference) / time_difference
    };

    accuracy as u16
}

#[derive(Clone, Deserialize, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    #[serde(deserialize_with = "i64_string::deserialize")]
    timestamp_ms: i64,
    latitude_e7: i64,
    longitude_e7: i64,
    accuracy: u16,
    activitys: Option<Vec<TimestampedActivity>>,
}

impl Location {
    pub fn coordinates(&self) -> coordinates::Coordinates {
        coordinates::Coordinates::new(
            self.latitude_e7 as f64 / 1e7,
            self.longitude_e7 as f64 / 1e7,
        )
    }

    pub fn accuracy(&self) -> u16 {
        self.accuracy
    }

    pub fn timestamp(&self) -> i64 {
        self.timestamp_ms / 1000 as i64
    }
}

#[derive(Clone, Deserialize, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
struct TimestampedActivity {
    #[serde(deserialize_with = "i64_string::deserialize")]
    timestamp_ms: i64,
    activities: Vec<Activity>,
    extras: Option<Vec<Extra>>,
}

#[derive(Clone, Deserialize, PartialEq, Debug)]
struct Activity {
    #[serde(rename = "type", deserialize_with = "activity_type_string::deserialize")]
    activity_type: ActivityType,
    confidence: u16,
}

#[derive(Clone, Deserialize, PartialEq, Debug)]
struct Extra {
    #[serde(rename = "type", deserialize_with = "extra_type_string::deserialize")]
    extra_type: ExtraType,
    #[serde(deserialize_with = "extra_name_string::deserialize")]
    name: ExtraName,
    #[serde(rename = "intVal")]
    int_val: u8,
}

#[derive(Clone, PartialEq, Debug)]
pub enum ActivityType {
    ExitingVehicle,
    InVehicle,
    OnBicycle,
    OnFoot,
    Running,
    Still,
    Tilting,
    Unknown,
    Walking,
    Other(String),
}

#[derive(Clone, PartialEq, Debug)]
pub enum ExtraType {
    Value,
    Other(String),
}

#[derive(Clone, PartialEq, Debug)]
pub enum ExtraName {
    VehiclePersonalConfidence,
    Other(String),
}

mod locations_sequence {
    use std::collections::BTreeMap;
    use std::iter::FromIterator;
    use serde::{Deserialize, Deserializer};
    use super::Location;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<BTreeMap<i64, Location>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let locations: Vec<Location> = Vec::deserialize(deserializer)?;

        Ok(BTreeMap::from_iter(
            locations.into_iter().map(|l| (l.timestamp_ms, l)),
        ))
    }
}

mod i64_string {
    use serde::{de, Deserialize, Deserializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<i64, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?.parse::<i64>().map_err(
            de::Error::custom,
        )
    }
}

mod activity_type_string {
    use super::ActivityType;
    use serde::{Deserialize, Deserializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<ActivityType, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(match String::deserialize(deserializer)?.as_ref() {
            "exitingVehicle" => ActivityType::ExitingVehicle,
            "inVehicle" => ActivityType::InVehicle,
            "onBicycle" => ActivityType::OnBicycle,
            "onFoot" => ActivityType::OnFoot,
            "running" => ActivityType::Running,
            "still" => ActivityType::Still,
            "tilting" => ActivityType::Tilting,
            "unknown" => ActivityType::Unknown,
            "walking" => ActivityType::Walking,
            x => ActivityType::Other(x.to_string()),
        })
    }
}

mod extra_type_string {
    use super::ExtraType;
    use serde::{Deserialize, Deserializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<ExtraType, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(match String::deserialize(deserializer)?.as_ref() {
            "value" => ExtraType::Value,
            x => ExtraType::Other(x.to_string()),
        })
    }
}

mod extra_name_string {
    use super::ExtraName;
    use serde::{Deserialize, Deserializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<ExtraName, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(match String::deserialize(deserializer)?.as_ref() {
            "vehicle_personal_confidence" => ExtraName::VehiclePersonalConfidence,
            x => ExtraName::Other(x.to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json;

    #[test]
    fn can_deserialize_a_google_location_history_file() {
        let s = r##"
            {
                "locations" : [ {
                    "timestampMs" : "1498358433377",
                    "latitudeE7" : 520796733,
                    "longitudeE7" : 11965831,
                    "accuracy" : 18,
                    "activitys" : [ {
                      "timestampMs" : "1498358433377",
                      "activities" : [ {
                        "type" : "still",
                        "confidence" : 100
                      } ],
                      "extras" : [ {
                        "type" : "value",
                        "name" : "vehicle_personal_confidence",
                        "intVal" : 100
                      } ]
                    } ]
                }, {
                    "timestampMs" : "1498358433377",
                    "latitudeE7" : 520796733,
                    "longitudeE7" : 11965831,
                    "accuracy" : 18,
                    "activitys" : [ {
                      "timestampMs" : "1498358433377",
                      "activities" : [ {
                        "type" : "still",
                        "confidence" : 100
                      } ]
                    } ]
                }, {
                    "timestampMs" : "1493657963571",
                    "latitudeE7" : 520567467,
                    "longitudeE7" : 11485831,
                    "accuracy" : 18
                } ]
            }
        "##;
        let glh: GoogleLocationHistory = serde_json::from_str(s).unwrap();

        let mut locations: BTreeMap<i64, Location> = BTreeMap::new();
        locations.insert(
            1498358433377,
            Location {
                timestamp_ms: 1498358433377,
                latitude_e7: 520796733,
                longitude_e7: 11965831,
                accuracy: 18,
                activitys: Some(vec![
                    TimestampedActivity {
                        timestamp_ms: 1498358433377,
                        activities: vec![
                            Activity {
                                activity_type: ActivityType::Still,
                                confidence: 100,
                            },
                        ],
                        extras: Some(vec![
                            Extra {
                                extra_type: ExtraType::Value,
                                name: ExtraName::VehiclePersonalConfidence,
                                int_val: 100,
                            },
                        ]),
                    },
                ]),
            },
        );
        locations.insert(
            1498358433377,
            Location {
                timestamp_ms: 1498358433377,
                latitude_e7: 520796733,
                longitude_e7: 11965831,
                accuracy: 18,
                activitys: Some(vec![
                    TimestampedActivity {
                        timestamp_ms: 1498358433377,
                        activities: vec![
                            Activity {
                                activity_type: ActivityType::Still,
                                confidence: 100,
                            },
                        ],
                        extras: None,
                    },
                ]),
            },
        );
        locations.insert(
            1493657963571,
            Location {
                timestamp_ms: 1493657963571,
                latitude_e7: 520567467,
                longitude_e7: 11485831,
                accuracy: 18,
                activitys: None,
            },
        );

        assert_eq!(glh, GoogleLocationHistory { locations });
    }

    #[test]
    fn get_most_likely_location_should_return_none_if_no_locations_exist() {
        let ghl = GoogleLocationHistory { locations: BTreeMap::new() };

        let location = ghl.get_most_likely_location(0);

        assert_eq!(None, location);
    }

    #[test]
    fn get_most_likely_location_should_return_the_location_with_a_matching_timestamp() {
        let mut locations: BTreeMap<i64, Location> = BTreeMap::new();
        locations.insert(
            1000,
            Location {
                timestamp_ms: 1000,
                latitude_e7: 520796733,
                longitude_e7: 11965831,
                accuracy: 18,
                activitys: None,
            },
        );
        let ghl = GoogleLocationHistory { locations };

        let location = ghl.get_most_likely_location(1).unwrap();

        assert_eq!(1000, location.timestamp_ms);
    }

    #[test]
    fn get_most_likely_location_should_return_the_location_at_the_closest_timestamp() {
        let mut locations: BTreeMap<i64, Location> = BTreeMap::new();
        locations.insert(
            3000,
            Location {
                timestamp_ms: 3000,
                latitude_e7: 520796733,
                longitude_e7: 11965831,
                accuracy: 18,
                activitys: None,
            },
        );
        locations.insert(
            6000,
            Location {
                timestamp_ms: 6000,
                latitude_e7: 520567467,
                longitude_e7: 11485831,
                accuracy: 18,
                activitys: None,
            },
        );
        let ghl = GoogleLocationHistory { locations };

        let location = ghl.get_most_likely_location(4).unwrap();

        assert_eq!(3000, location.timestamp_ms);
    }

    #[test]
    fn get_most_likely_location_should_return_the_older_location_if_exactly_between_two() {
        let mut locations: BTreeMap<i64, Location> = BTreeMap::new();
        locations.insert(
            1000,
            Location {
                timestamp_ms: 1000,
                latitude_e7: 520796733,
                longitude_e7: 11965831,
                accuracy: 18,
                activitys: None,
            },
        );
        locations.insert(
            3000,
            Location {
                timestamp_ms: 3000,
                latitude_e7: 520796733,
                longitude_e7: 11965831,
                accuracy: 18,
                activitys: None,
            },
        );
        let ghl = GoogleLocationHistory { locations };

        let location = ghl.get_most_likely_location(2).unwrap();

        assert_eq!(1000, location.timestamp_ms);
    }

    #[test]
    fn interpolate_location_should_return_none_if_no_locations_exist() {
        let ghl = GoogleLocationHistory { locations: BTreeMap::new() };

        let location = ghl.interpolate_location(0);

        assert_eq!(None, location);
    }

    #[test]
    fn interpolate_location_should_return_the_location_with_a_matching_timestamp() {
        let mut locations: BTreeMap<i64, Location> = BTreeMap::new();
        locations.insert(
            1000,
            Location {
                timestamp_ms: 1000,
                latitude_e7: 520796733,
                longitude_e7: 11965831,
                accuracy: 18,
                activitys: None,
            },
        );
        let ghl = GoogleLocationHistory { locations };

        let location = ghl.interpolate_location(1).unwrap();

        assert_eq!(1000, location.timestamp_ms);
    }

    #[test]
    fn interpolate_location_should_linearly_interpolate_between_positions() {
        let mut locations: BTreeMap<i64, Location> = BTreeMap::new();
        locations.insert(
            3000,
            Location {
                timestamp_ms: 3000,
                latitude_e7: 520796733,
                longitude_e7: 11965831,
                accuracy: 18,
                activitys: None,
            },
        );
        locations.insert(
            6000,
            Location {
                timestamp_ms: 6000,
                latitude_e7: 520567467,
                longitude_e7: 11485831,
                accuracy: 20,
                activitys: None,
            },
        );
        let ghl = GoogleLocationHistory { locations };

        let location = ghl.interpolate_location(4).unwrap();

        assert_eq!(4000, location.timestamp_ms);
        assert_eq!(520720311, location.latitude_e7);
        assert_eq!(11805831, location.longitude_e7);
        assert_eq!(1339, location.accuracy);
        assert!(location.activitys.is_none());
    }

    #[test]
    fn interpolate_accuracy_should_use_only_location_accuracies_with_small_half_distance() {
        let accuracy = interpolate_accuracy(
            4000,
            &Location {
                timestamp_ms: 3000,
                latitude_e7: 520796733,
                longitude_e7: 11965831,
                accuracy: 10,
                activitys: None,
            },
            &Location {
                timestamp_ms: 7000,
                latitude_e7: 520796734,
                longitude_e7: 11965831,
                accuracy: 20,
                activitys: None,
            },
        );

        assert_eq!(12, accuracy);

        let accuracy = interpolate_accuracy(
            4000,
            &Location {
                timestamp_ms: 3000,
                latitude_e7: 520796733,
                longitude_e7: 11965831,
                accuracy: 20,
                activitys: None,
            },
            &Location {
                timestamp_ms: 7000,
                latitude_e7: 520796734,
                longitude_e7: 11965831,
                accuracy: 10,
                activitys: None,
            },
        );

        assert_eq!(18, accuracy);
    }

    #[test]
    fn interpolate_accuracy_should_scale_to_half_distance_at_mid_point() {
        let before = Location {
            timestamp_ms: 3000,
            latitude_e7: 520796733,
            longitude_e7: 11965831,
            accuracy: 10,
            activitys: None,
        };
        let after = Location {
            timestamp_ms: 7000,
            latitude_e7: 520567467,
            longitude_e7: 11485831,
            accuracy: 30,
            activitys: None,
        };

        let accuracy = interpolate_accuracy(3500, &before, &after);
        assert_eq!(507, accuracy);

        let accuracy = interpolate_accuracy(5000, &before, &after);
        assert_eq!(2000, accuracy);

        let before = Location {
            timestamp_ms: 3000,
            latitude_e7: 520796733,
            longitude_e7: 11965831,
            accuracy: 4000,
            activitys: None,
        };

        let accuracy = interpolate_accuracy(3500, &before, &after);
        assert_eq!(3500, accuracy);
    }

    #[test]
    fn interpolate_accuracy_should_scale_from_half_distance_at_mid_point() {
        let before = Location {
            timestamp_ms: 3000,
            latitude_e7: 520796733,
            longitude_e7: 11965831,
            accuracy: 10,
            activitys: None,
        };
        let after = Location {
            timestamp_ms: 7000,
            latitude_e7: 520567467,
            longitude_e7: 11485831,
            accuracy: 300,
            activitys: None,
        };

        let accuracy = interpolate_accuracy(5500, &before, &after);
        assert_eq!(1575, accuracy);

        let after = Location {
            timestamp_ms: 7000,
            latitude_e7: 520567467,
            longitude_e7: 11485831,
            accuracy: 3000,
            activitys: None,
        };

        let accuracy = interpolate_accuracy(5500, &before, &after);
        assert_eq!(2250, accuracy);
    }

    #[test]
    fn coordinates_should_return_the_location_coordinates() {
        let location = Location {
            timestamp_ms: 1000,
            latitude_e7: 520796733,
            longitude_e7: 11965831,
            accuracy: 18,
            activitys: None,
        };

        let coordinates = location.coordinates();

        assert_eq!(52.0796733, coordinates.latitude());
        assert_eq!(1.1965831, coordinates.longitude());
    }
}