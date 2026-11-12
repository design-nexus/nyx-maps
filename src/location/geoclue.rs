use anyhow::{Result, bail};
use std::time::Duration;
use zbus::Connection;
use zbus::zvariant::OwnedObjectPath;

#[derive(Debug, Clone)]
pub struct GeoLocation {
    pub latitude: f64,
    pub longitude: f64,
    pub accuracy: f64,
    pub label: Option<String>,
}

pub async fn request_current_location() -> Result<GeoLocation> {
    // 1. Try GeoClue2 system service with a strict 1s timeout so we never block the UI
    match tokio::time::timeout(Duration::from_millis(1000), request_geoclue_location()).await {
        Ok(Ok(loc)) => Ok(loc),
        _ if crate::settings::get().ip_location => request_ip_location().await,
        _ => bail!("The location service didn't answer. Turn on Location in the system settings, or let Maps guess from your IP address in its settings."),
    }
}

async fn request_geoclue_location() -> Result<GeoLocation> {
    let connection = Connection::system().await?;

    let reply = connection
        .call_method(
            Some("org.freedesktop.GeoClue2"),
            "/org/freedesktop/GeoClue2/Manager",
            Some("org.freedesktop.GeoClue2.Manager"),
            "GetClient",
            &(),
        )
        .await?;

    let client_path: OwnedObjectPath = reply.body().deserialize()?;

    // Set DesktopId
    let _ = connection
        .call_method(
            Some("org.freedesktop.GeoClue2"),
            client_path.as_str(),
            Some("org.freedesktop.DBus.Properties"),
            "Set",
            &(
                "org.freedesktop.GeoClue2.Client",
                "DesktopId",
                zbus::zvariant::Value::from("io.github.design_nexus.nyx.Maps"),
            ),
        )
        .await;

    // Set RequestedAccuracyLevel to 4 (city level)
    let _ = connection
        .call_method(
            Some("org.freedesktop.GeoClue2"),
            client_path.as_str(),
            Some("org.freedesktop.DBus.Properties"),
            "Set",
            &(
                "org.freedesktop.GeoClue2.Client",
                "RequestedAccuracyLevel",
                zbus::zvariant::Value::from(4u32),
            ),
        )
        .await;

    // Start client
    connection
        .call_method(
            Some("org.freedesktop.GeoClue2"),
            client_path.as_str(),
            Some("org.freedesktop.GeoClue2.Client"),
            "Start",
            &(),
        )
        .await?;

    // Poll for Location property for up to 800ms
    let mut location_path: Option<OwnedObjectPath> = None;
    for _ in 0..4 {
        tokio::time::sleep(Duration::from_millis(200)).await;

        let prop_reply = connection
            .call_method(
                Some("org.freedesktop.GeoClue2"),
                client_path.as_str(),
                Some("org.freedesktop.DBus.Properties"),
                "Get",
                &("org.freedesktop.GeoClue2.Client", "Location"),
            )
            .await;

        if let Ok(reply) = prop_reply {
            if let Ok(variant) = reply.body().deserialize::<zbus::zvariant::OwnedValue>() {
                if let Ok(path) = OwnedObjectPath::try_from(variant) {
                    if path.as_str() != "/" {
                        location_path = Some(path);
                        break;
                    }
                }
            }
        }
    }

    // Stop client to save power
    let _ = connection
        .call_method(
            Some("org.freedesktop.GeoClue2"),
            client_path.as_str(),
            Some("org.freedesktop.GeoClue2.Client"),
            "Stop",
            &(),
        )
        .await;

    let loc_path = location_path.ok_or_else(|| anyhow::anyhow!("No location fix from GeoClue"))?;

    let lat_reply = connection
        .call_method(
            Some("org.freedesktop.GeoClue2"),
            loc_path.as_str(),
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &("org.freedesktop.GeoClue2.Location", "Latitude"),
        )
        .await?;
    let lat: f64 = lat_reply.body().deserialize::<zbus::zvariant::OwnedValue>()?.try_into().unwrap_or(0.0);

    let lon_reply = connection
        .call_method(
            Some("org.freedesktop.GeoClue2"),
            loc_path.as_str(),
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &("org.freedesktop.GeoClue2.Location", "Longitude"),
        )
        .await?;
    let lon: f64 = lon_reply.body().deserialize::<zbus::zvariant::OwnedValue>()?.try_into().unwrap_or(0.0);

    let acc_reply = connection
        .call_method(
            Some("org.freedesktop.GeoClue2"),
            loc_path.as_str(),
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &("org.freedesktop.GeoClue2.Location", "Accuracy"),
        )
        .await?;
    let acc: f64 = acc_reply.body().deserialize::<zbus::zvariant::OwnedValue>()?.try_into().unwrap_or(100.0);

    Ok(GeoLocation {
        latitude: lat,
        longitude: lon,
        accuracy: acc,
        label: Some("Current Location".to_string()),
    })
}

async fn request_ip_location() -> Result<GeoLocation> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()?;

    // Try ipwho.is first
    if let Ok(res) = client.get("https://ipwho.is/").send().await {
        if let Ok(v) = res.json::<serde_json::Value>().await {
            if v["success"].as_bool() == Some(true) {
                if let (Some(lat), Some(lon)) = (v["latitude"].as_f64(), v["longitude"].as_f64()) {
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
                        label: Some(label),
                    });
                }
            }
        }
    }

    // Fallback: ipapi.co
    if let Ok(res) = client.get("https://ipapi.co/json/").send().await {
        if let Ok(v) = res.json::<serde_json::Value>().await {
            if let (Some(lat), Some(lon)) = (v["latitude"].as_f64(), v["longitude"].as_f64()) {
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
                    label: Some(label),
                });
            }
        }
    }

    bail!("Could not determine location from GeoClue or network IP")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_request_current_location() {
        // Without GeoClue, and with the IP guess off by default, it says why instead.
        match request_current_location().await {
            Ok(l) => assert!(l.latitude.is_finite() && l.longitude.is_finite() && l.label.is_some()),
            Err(e) => assert!(e.to_string().contains("location service"), "{e}"),
        }
    }
}
