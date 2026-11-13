use crate::providers::{TravelMode, calculate_route, parse_coordinates, search_nearby, search_places};
use crate::settings;
use serde_json::json;

pub async fn handle_cli_api(args: &[String]) -> bool {
    if args.is_empty() {
        return false;
    }

    if args[0] != "--api" {
        return false;
    }

    if args.len() < 2 {
        eprintln!("{}", json!({ "error": "Missing API command. Use 'search', 'nearby', 'distance', or 'route'." }));
        return true;
    }

    let cmd = &args[1];
    let api_key = settings::get().geoapify_api_key;

    match cmd.as_str() {
        "search" => {
            if args.len() < 3 {
                println!("{}", json!({ "error": "Missing search query" }));
                return true;
            }
            let query = &args[2];
            match search_places(query, &api_key).await {
                Ok(results) => {
                    println!("{}", serde_json::to_string_pretty(&results).unwrap());
                }
                Err(e) => {
                    println!("{}", json!({ "error": e.to_string() }));
                }
            }
            true
        }
        "nearby" => {
            if args.len() < 4 {
                println!("{}", json!({ "error": "Usage: --api nearby <category> <lat,lon> [radius_km]" }));
                return true;
            }
            let category = &args[2];
            let coords = parse_coordinates(&args[3]);
            let radius = if args.len() >= 5 {
                args[4].parse::<f64>().unwrap_or(8.0)
            } else {
                8.0
            };
            if let Some((lat, lon)) = coords {
                match search_nearby(category, lat, lon, radius, &api_key).await {
                    Ok(results) => {
                        println!("{}", serde_json::to_string_pretty(&results).unwrap());
                    }
                    Err(e) => {
                        println!("{}", json!({ "error": e.to_string() }));
                    }
                }
            } else {
                println!("{}", json!({ "error": "Invalid coordinates" }));
            }
            true
        }
        "distance" | "route" => {
            if args.len() < 4 {
                println!("{}", json!({ "error": "Usage: --api distance/route <start_lat,start_lon> <end_lat,end_lon> [mode]" }));
                return true;
            }
            let start = parse_coordinates(&args[2]);
            let end = parse_coordinates(&args[3]);
            let mode = if args.len() >= 5 {
                TravelMode::from_str(&args[4])
            } else {
                TravelMode::Drive
            };

            if let (Some(s), Some(e)) = (start, end) {
                match calculate_route(s, e, mode, &api_key).await {
                    Ok(route) => {
                        if cmd == "distance" {
                            println!("{}", json!({
                                "mode": route.mode.display_name(),
                                "distance_meters": route.distance_meters,
                                "duration_seconds": route.duration_seconds,
                                "formatted_distance": crate::providers::format_distance(route.distance_meters, &settings::get().units),
                                "formatted_duration": crate::providers::format_duration(route.duration_seconds),
                            }));
                        } else {
                            println!("{}", serde_json::to_string_pretty(&route).unwrap());
                        }
                    }
                    Err(err) => {
                        println!("{}", json!({ "error": err.to_string() }));
                    }
                }
            } else {
                println!("{}", json!({ "error": "Invalid start or end coordinates. Expected 'lat,lon'" }));
            }
            true
        }
        "locate" => {
            // One line per fix, each better than the last.
            let (sender, receiver) = async_channel::unbounded();
            let finding = crate::location::locate(sender, crate::location::PATIENCE);
            let printing = async {
                while let Ok(fix) = receiver.recv().await {
                    match fix {
                        Ok(f) => println!(
                            "{}",
                            json!({
                                "latitude": f.latitude,
                                "longitude": f.longitude,
                                "accuracy_meters": f.accuracy,
                                "from_ip": f.from_ip,
                                "accuracy": crate::location::accuracy_text(f.accuracy, f.from_ip, &settings::get().units),
                            })
                        ),
                        Err(e) => println!("{}", json!({ "error": e.to_string() })),
                    }
                }
            };
            tokio::join!(finding, printing);
            true
        }
        _ => {
            println!("{}", json!({ "error": format!("Unknown API command '{}'", cmd) }));
            true
        }
    }
}
