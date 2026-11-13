use crate::providers::format_distance;
use anyhow::{Result, anyhow, bail};
use async_channel::Sender;
use std::time::{Duration, Instant};
use zbus::Connection;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

#[derive(Debug, Clone)]
pub struct GeoLocation {
    pub latitude: f64,
    pub longitude: f64,
    /// Metres, as the source states it.
    pub accuracy: f64,
    /// Guessed from the internet address rather than Wi-Fi or GPS.
    pub from_ip: bool,
    pub label: Option<String>,
}

const GEOCLUE: &str = "org.freedesktop.GeoClue2";
const CLIENT: &str = "org.freedesktop.GeoClue2.Client";
/// GeoClue's GCLUE_ACCURACY_LEVEL_EXACT: every source, Wi-Fi and GPS included.
const ACCURACY_EXACT: u32 = 8;
/// A fix this close ends the search early.
const GOOD_ENOUGH_M: f64 = 50.0;
/// How long GeoClue gets to improve on its first fix when you ask for your location
/// (the Wi-Fi scan and its lookup take a few seconds).
pub const PATIENCE: Duration = Duration::from_secs(20);

/// Finds where you are and sends each better fix as it comes: GeoClue often answers
/// first with its internet-address guess, then again once a Wi-Fi scan is looked up.
/// Without GeoClue: one guess from the IP address if Settings allow it, else why not.
pub async fn locate(fixes: Sender<Result<GeoLocation>>, patience: Duration) {
    let found = match geoclue_fixes(&fixes, patience).await {
        Ok(found) => found,
        Err(e) => {
            eprintln!("nyx-maps: location service: {e:#}");
            false
        }
    };
    if found {
        return;
    }
    let fallback = if crate::settings::get().ip_location {
        request_ip_location().await
    } else {
        Err(anyhow!("The location service didn't answer. Turn on Location in the system settings, or let Maps guess from your IP address in its settings."))
    };
    let _ = fixes.send(fallback).await;
}

/// The best fix within `patience`, for callers that need a single answer.
pub async fn request_current_location(patience: Duration) -> Result<GeoLocation> {
    let (sender, receiver) = async_channel::unbounded();
    locate(sender, patience).await;
    let mut best = None;
    while let Ok(fix) = receiver.try_recv() {
        best = Some(fix);
    }
    best.unwrap_or_else(|| Err(anyhow!("No location")))
}

/// How close a fix is, in words: "Within 30 m", or where a rough one comes from.
pub fn accuracy_text(accuracy: f64, from_ip: bool, units: &str) -> String {
    if from_ip {
        "Approximate, from your internet address".to_string()
    } else if accuracy <= 1000.0 {
        format!("Within {}", format_distance(accuracy, units))
    } else {
        format!("Approximate, within {}", format_distance(accuracy, units))
    }
}

/// True once at least one fix was sent.
async fn geoclue_fixes(fixes: &Sender<Result<GeoLocation>>, patience: Duration) -> Result<bool> {
    let bus = Connection::system().await?;
    let reply = bus
        .call_method(Some(GEOCLUE), "/org/freedesktop/GeoClue2/Manager", Some("org.freedesktop.GeoClue2.Manager"), "GetClient", &())
        .await?;
    let client: OwnedObjectPath = reply.body().deserialize()?;
    set_property(&bus, &client, "DesktopId", Value::from("io.github.design_nexus.nyx.Maps")).await?;
    set_property(&bus, &client, "RequestedAccuracyLevel", Value::from(ACCURACY_EXACT)).await?;
    // A GeoClue that has just been started waits up to 5 s for the desktop's agent
    // before it answers Start.
    tokio::time::timeout(Duration::from_secs(8), bus.call_method(Some(GEOCLUE), client.as_str(), Some(CLIENT), "Start", &()))
        .await
        .map_err(|_| anyhow!("GeoClue didn't start the client"))??;
    let found = follow(&bus, &client, fixes, patience).await;
    let _ = bus.call_method(Some(GEOCLUE), client.as_str(), Some(CLIENT), "Stop", &()).await;
    found
}

/// Sends each fix better than the last until one is good enough or time is up.
async fn follow(bus: &Connection, client: &OwnedObjectPath, fixes: &Sender<Result<GeoLocation>>, patience: Duration) -> Result<bool> {
    let deadline = Instant::now() + patience;
    let mut seen: Option<OwnedObjectPath> = None;
    let mut best = f64::INFINITY;
    while Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(250)).await;
        let path = OwnedObjectPath::try_from(get_property(bus, client, CLIENT, "Location").await?)?;
        if path.as_str() == "/" || seen.as_ref() == Some(&path) {
            continue;
        }
        seen = Some(path.clone());
        let fix = read_fix(bus, &path).await?;
        if fix.accuracy < best {
            best = fix.accuracy;
            if fixes.send(Ok(fix)).await.is_err() || best <= GOOD_ENOUGH_M {
                break;
            }
        }
    }
    Ok(best.is_finite())
}

async fn read_fix(bus: &Connection, path: &OwnedObjectPath) -> Result<GeoLocation> {
    const LOCATION: &str = "org.freedesktop.GeoClue2.Location";
    let number = |v: OwnedValue| f64::try_from(v).map_err(anyhow::Error::from);
    let latitude = number(get_property(bus, path, LOCATION, "Latitude").await?)?;
    let longitude = number(get_property(bus, path, LOCATION, "Longitude").await?)?;
    let accuracy = number(get_property(bus, path, LOCATION, "Accuracy").await?)?;
    let description = match get_property(bus, path, LOCATION, "Description").await {
        Ok(v) => String::try_from(v).unwrap_or_default(),
        Err(_) => String::new(),
    };
    Ok(GeoLocation { latitude, longitude, accuracy, from_ip: is_ip_source(&description), label: None })
}

/// GeoClue's own words for a fix: "GeoIP (ichnaea)", or BeaconDB's "ipf fallback
/// (from WiFi data)" when it knows none of the access points around.
fn is_ip_source(description: &str) -> bool {
    let d = description.to_ascii_lowercase();
    d.contains("geoip") || d.starts_with("ipf")
}

async fn get_property(bus: &Connection, path: &OwnedObjectPath, interface: &str, name: &str) -> Result<OwnedValue> {
    let reply = bus
        .call_method(Some(GEOCLUE), path.as_str(), Some("org.freedesktop.DBus.Properties"), "Get", &(interface, name))
        .await?;
    Ok(reply.body().deserialize::<OwnedValue>()?)
}

async fn set_property(bus: &Connection, path: &OwnedObjectPath, name: &str, value: Value<'_>) -> Result<()> {
    bus.call_method(Some(GEOCLUE), path.as_str(), Some("org.freedesktop.DBus.Properties"), "Set", &(CLIENT, name, value))
        .await?;
    Ok(())
}

async fn request_ip_location() -> Result<GeoLocation> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()?;

    // Try ipwho.is first
    if let Ok(res) = client.get("https://ipwho.is/").send().await
        && let Ok(v) = res.json::<serde_json::Value>().await
        && v["success"].as_bool() == Some(true)
        && let (Some(lat), Some(lon)) = (v["latitude"].as_f64(), v["longitude"].as_f64())
    {
        let city = v["city"].as_str().unwrap_or("");
        let region = v["region"].as_str().unwrap_or("");
        let label = if !city.is_empty() && !region.is_empty() {
            format!("{city}, {region}")
        } else if !city.is_empty() {
            city.to_string()
        } else {
            "Current Location".to_string()
        };
        return Ok(GeoLocation {
            latitude: lat,
            longitude: lon,
            accuracy: 5000.0,
            from_ip: true,
            label: Some(label),
        });
    }

    // Fallback: ipapi.co
    if let Ok(res) = client.get("https://ipapi.co/json/").send().await
        && let Ok(v) = res.json::<serde_json::Value>().await
        && let (Some(lat), Some(lon)) = (v["latitude"].as_f64(), v["longitude"].as_f64())
    {
        let city = v["city"].as_str().unwrap_or("");
        let region = v["region"].as_str().unwrap_or("");
        let label = if !city.is_empty() && !region.is_empty() {
            format!("{city}, {region}")
        } else {
            "Current Location".to_string()
        };
        return Ok(GeoLocation {
            latitude: lat,
            longitude: lon,
            accuracy: 10000.0,
            from_ip: true,
            label: Some(label),
        });
    }

    bail!("Could not determine location from GeoClue or network IP")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ip_sources() {
        assert!(is_ip_source("GeoIP (ichnaea)"));
        assert!(is_ip_source("ipf fallback (from WiFi data)"));
        assert!(!is_ip_source("WiFi"));
        assert!(!is_ip_source(""));
    }

    #[test]
    fn accuracy_words() {
        assert_eq!(accuracy_text(30.0, false, "metric"), "Within 30 m");
        assert_eq!(accuracy_text(25_000.0, false, "metric"), "Approximate, within 25.0 km");
        assert_eq!(accuracy_text(25_000.0, true, "metric"), "Approximate, from your internet address");
    }
}
