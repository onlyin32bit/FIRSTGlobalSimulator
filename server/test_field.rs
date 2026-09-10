use std::path::Path;

fn main() {
    let manifest_path = "D:/Robotics/FIRSTGlobalSimulator/pkgs/games/fgc-2026/manifest.json";
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(manifest_path).unwrap(),
    ).unwrap();
    let physics_path = manifest["field"]["physics"].as_str().unwrap();
    let physics = serde_json::from_str::<serde_json::Value>(
        &std::fs::read_to_string(Path::new(manifest_path).parent().unwrap().join(physics_path)).unwrap(),
    ).unwrap();
    
    let root = physics["rootnode"].as_object().unwrap();
    if let Some(children) = root.get("children").and_then(|c| c.as_array()) {
        for child in children {
            let name = child["name"].as_str().unwrap();
            println!("{}", name);
        }
    }
}
