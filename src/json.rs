use std::collections::BTreeMap;

#[derive(Deserialize, PartialEq, Debug)]
struct GoogleLocationHistory {
    #[serde(deserialize_with = "locations_sequence::deserialize")]
    locations: BTreeMap<i64, Location>,
}

#[derive(Deserialize, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    #[serde(deserialize_with = "i64_string::deserialize")]
    timestamp_ms: i64,
    latitude_e7: i64,
    longitude_e7: i64,
    accuracy: u16,
    activitys: Option<Vec<TimestampedActivity>>,
}

#[derive(Deserialize, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
struct TimestampedActivity {
    #[serde(deserialize_with = "i64_string::deserialize")]
    timestamp_ms: i64,
    activities: Vec<Activity>,
    extras: Option<Vec<Extra>>,
}

#[derive(Deserialize, PartialEq, Debug)]
struct Activity {
    #[serde(rename = "type", deserialize_with = "activity_type_string::deserialize")]
    activity_type: ActivityType,
    confidence: u16,
}

#[derive(Deserialize, PartialEq, Debug)]
struct Extra {
    #[serde(rename = "type", deserialize_with = "extra_type_string::deserialize")]
    extra_type: ExtraType,
    #[serde(deserialize_with = "extra_name_string::deserialize")]
    name: ExtraName,
    #[serde(rename = "intVal")]
    int_val: u8,
}

#[derive(PartialEq, Debug)]
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

#[derive(PartialEq, Debug)]
pub enum ExtraType {
    Value,
    Other(String),
}

#[derive(PartialEq, Debug)]
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
        where D: Deserializer<'de> {
        let locations: Vec<Location> = Vec::deserialize(deserializer)?;

        Ok(BTreeMap::from_iter(locations.into_iter().map(|l| (l.timestamp_ms, l))))
    }
}

mod i64_string {
    use serde::{de, Deserialize, Deserializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<i64, D::Error>
        where D: Deserializer<'de> {
        String::deserialize(deserializer)?.parse::<i64>().map_err(de::Error::custom)
    }
}

mod activity_type_string {
    use super::ActivityType;
    use serde::{Deserialize, Deserializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<ActivityType, D::Error>
        where D: Deserializer<'de> {
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
        where D: Deserializer<'de> {
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
        where D: Deserializer<'de> {
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
                    "latitudeE7" : 5207967334,
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
                    "latitudeE7" : 5207967334,
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
                    "latitudeE7" : 5205674674,
                    "longitudeE7" : 11485831,
                    "accuracy" : 18
                } ]
            }
        "##;
        let glh: GoogleLocationHistory = serde_json::from_str(s).unwrap();

        let mut locations: BTreeMap<i64, Location> = BTreeMap::new();
        locations.insert(1498358433377, Location {
            timestamp_ms: 1498358433377,
            latitude_e7: 5207967334,
            longitude_e7: 11965831,
            accuracy: 18,
            activitys: Some(vec![TimestampedActivity {
                timestamp_ms: 1498358433377,
                activities: vec![Activity {
                    activity_type: ActivityType::Still,
                    confidence: 100,
                }],
                extras: Some(vec![Extra {
                    extra_type: ExtraType::Value,
                    name: ExtraName::VehiclePersonalConfidence,
                    int_val: 100,
                }]),
            }]),
        });
        locations.insert(1498358433377, Location {
            timestamp_ms: 1498358433377,
            latitude_e7: 5207967334,
            longitude_e7: 11965831,
            accuracy: 18,
            activitys: Some(vec![TimestampedActivity {
                timestamp_ms: 1498358433377,
                activities: vec![Activity {
                    activity_type: ActivityType::Still,
                    confidence: 100,
                }],
                extras: None,
            }]),
        });
        locations.insert(1493657963571, Location {
            timestamp_ms: 1493657963571,
            latitude_e7: 5205674674,
            longitude_e7: 11485831,
            accuracy: 18,
            activitys: None,
        });

        assert_eq!(glh, GoogleLocationHistory { locations });
    }
}
