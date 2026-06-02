use notify::{RecursiveMode, Result as NotifyResult, Watcher};
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

enum AppEvent {
    FileActivity(PathBuf),
    SkipRequested,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let replay_folder = r"C:\Users\[username]\Documents\TmForever\Tracks\Replays\Autosaves";
    let download_target = r"C:\Users\[username]\Desktop\TestTracks";

    println!("--- TrackMania Randomizer Bot Started ---");
    println!("Monitoring for file activity in: {}", replay_folder);
    println!("Type 's' and press Enter to skip a map.");
    println!("-----------------------------------------\n");

    let (tx, rx) = channel::<AppEvent>();

    // PRODUCER 1: File Watcher
    let tx_watcher = tx.clone();
    let mut watcher = notify::recommended_watcher(move |res: NotifyResult<notify::Event>| {
        if let Ok(event) = res {
            for path in event.paths {
                if path
                    .extension()
                    .and_then(|s| s.to_str())
                    .map(|s| s.eq_ignore_ascii_case("gbx"))
                    == Some(true)
                {
                    let _ = tx_watcher.send(AppEvent::FileActivity(path));
                }
            }
        }
    })?;
    watcher.watch(Path::new(replay_folder), RecursiveMode::NonRecursive)?;

    // PRODUCER 2: Keyboard Monitor
    let tx_stdin = tx.clone();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut input = String::new();
        loop {
            input.clear();
            if stdin.read_line(&mut input).is_ok() {
                if input.trim().eq_ignore_ascii_case("s") {
                    let _ = tx_stdin.send(AppEvent::SkipRequested);
                }
            }
        }
    });

    // --- STATE TRACKING ---
    let mut current_author_time: i32 = 0;
    let mut recent_files: HashMap<String, Instant> = HashMap::new();

    // STARTUP
    println!("Fetching initial map...");
    match download_next_map(download_target) {
        Ok(time) => current_author_time = time,
        Err(e) => eprintln!(
            "Failed to download initial map: {}. Type 's' to try again.",
            e
        ),
    }

    // MAIN LOOP
    for app_event in rx {
        match app_event {
            AppEvent::FileActivity(path) => {
                // Anti-Spam: Ignore this file if we already processed it in the last 5 seconds
                let filename = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                if let Some(&last_seen) = recent_files.get(&filename) {
                    if last_seen.elapsed().as_secs() < 5 {
                        continue;
                    }
                }

                // Record that we are processing this file now
                recent_files.insert(filename, Instant::now());

                // Give TM a second to finish writing the file to disk
                std::thread::sleep(Duration::from_millis(1000));

                if current_author_time == 0 {
                    println!("A replay was saved, but no active author time is set. Skipping.");
                    continue;
                }

                // Read the Replay file to get your time
                match get_replay_best_time(&path) {
                    Ok(best_time) => {
                        println!(
                            "   -> Your Time: {}, Author Target: {}",
                            best_time, current_author_time
                        );

                        if best_time <= current_author_time && best_time > 0 {
                            println!("\n🏆 Author Medal beat! Downloading next map...");
                            match download_next_map(download_target) {
                                Ok(time) => current_author_time = time,
                                Err(e) => eprintln!("Failed to download next map: {}", e),
                            }
                        } else {
                            println!("❌ Close, but not quite! Keep trying.");
                        }
                    }
                    Err(e) => eprintln!("Error reading replay file: {}", e),
                }
            }
            AppEvent::SkipRequested => {
                println!("\n⏭️ Skip requested! Downloading next map...");
                match download_next_map(download_target) {
                    Ok(time) => current_author_time = time,
                    Err(e) => eprintln!("Failed to download map: {}", e),
                }
            }
        }
    }

    Ok(())
}

/// Reads the .Replay.Gbx file to find the player's time
fn get_replay_best_time(path: &Path) -> Result<i32, Box<dyn std::error::Error>> {
    let bytes = fs::read(path)?;
    let content = String::from_utf8_lossy(&bytes);

    let re_best = Regex::new(r#"best="(\d+)""#)?;

    let best_time = re_best
        .captures(&content)
        .and_then(|cap| cap.get(1))
        .ok_or("Best time not found in replay file. Game may still be writing it.")?
        .as_str()
        .parse::<i32>()?;

    Ok(best_time)
}

/// Downloads the map, parses the map file for the Author Time, and returns it
fn download_next_map(target_folder: &str) -> Result<i32, Box<dyn std::error::Error>> {
    // 1. Get a random track ID
    let response = reqwest::blocking::get("https://tmnf.exchange/trackrandom")?;
    let track_id = response.url().as_str().split('/').last().unwrap();

    // 2. Download the track file
    let download_url = format!("https://tmnf.exchange/trackgbx/{}", track_id);
    let response = reqwest::blocking::get(&download_url)?;

    // We grab the raw bytes into memory so we can both save it AND read it
    let bytes = response.bytes()?;

    // 3. Save it to disk
    let file_path = Path::new(target_folder).join(format!("next_map_{}.gbx", track_id));
    let mut file = std::fs::File::create(&file_path)?;
    let mut content = Cursor::new(bytes.clone());
    std::io::copy(&mut content, &mut file)?;

    // 4. Extract the Author time from the newly downloaded map file
    let content_str = String::from_utf8_lossy(&bytes);
    let re_author = Regex::new(r#"authortime="(\d+)""#)?;

    let author_time = re_author
        .captures(&content_str)
        .and_then(|cap| cap.get(1))
        .ok_or("Author time not found in downloaded map file")?
        .as_str()
        .parse::<i32>()?;

    println!(
        "Downloaded Map ID {}. Target to beat: {} ms",
        track_id, author_time
    );

    // 5. Open it in-game
    opener::open(&file_path)?;

    Ok(author_time)
}
