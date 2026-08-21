
use std::fs;
use serde_json::Value;

fn main() {
    let json_str = fs::read_to_string("../pkgs/games/fgc-2026/robots/starter-bot/bot.semantics.json").unwrap();
    let scene: Value = serde_json::from_str(&json_str).unwrap();
    
    // implement just the bits we need
    let outtake_target = scene["rootnode"]["children"].as_array().unwrap().iter().find(|n| n["name"] == "OuttakeTarget").unwrap();
    let outtake_zone = scene["rootnode"]["children"].as_array().unwrap().iter().find(|n| n["name"] == "OuttakeZone").unwrap();
    
    let t_m = outtake_target["transformation"].as_array().unwrap();
    let z_m = outtake_zone["transformation"].as_array().unwrap();
    
    println!("Target Matrix: {:?}", t_m.iter().map(|v| v.as_f64().unwrap()).collect::<Vec<_>>());
    println!("Zone Matrix: {:?}", z_m.iter().map(|v| v.as_f64().unwrap()).collect::<Vec<_>>());
}

