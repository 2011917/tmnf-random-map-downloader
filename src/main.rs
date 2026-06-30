use notify::{RecursiveMode, Result as NotifyResult, Watcher};
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

// ==========================================
// APP STATE & EVENTS
// ==========================================
enum AppEvent {
    Input(String),
    FileActivity(PathBuf),
}

#[derive(Clone, PartialEq)]
enum PlayMode {
    BeatAuthor,
    Grinding,
}

#[derive(Clone)]
enum AppState {
    MainMenu,
    RandomInfiniteMenu,
    ChallengeMenu,
    ChallengeCustomTimeInput,
    ChooseMapInput,
    Playing {
        mode: PlayMode,
        current_track_id: String,
        current_author_time: i32,
        challenge_end: Option<Instant>,
        maps_beaten: u32,
        last_replay_path: Option<PathBuf>,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ------------------------------------------
    // NATIVE OS PATH RESOLUTION
    // ------------------------------------------
    let replay_folder = dirs::document_dir()
        .expect("❌ Could not find OS Documents folder")
        .join("TmForever")
        .join("Tracks")
        .join("Replays")
        .join("Autosaves");

    let target_folder = dirs::desktop_dir()
        .expect("❌ Could not find OS Desktop folder")
        .join("TestTracks");

    if !replay_folder.exists() {
        eprintln!(
            "❌ [ERROR] Replay folder DOES NOT EXIST: {}",
            replay_folder.display()
        );
        eprintln!("Please check that TrackMania has created the Autosaves folder.");
        return Ok(());
    }
    if !target_folder.exists() {
        std::fs::create_dir_all(&target_folder)?;
    }

    let (tx, rx) = channel::<AppEvent>();

    // ------------------------------------------
    // THREAD 1: File Watcher
    // ------------------------------------------
    let tx_watcher = tx.clone();
    let watch_path = replay_folder.clone();

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

    watcher.watch(&watch_path, RecursiveMode::NonRecursive)?;

    // ------------------------------------------
    // THREAD 2: Keyboard Input Monitor
    // ------------------------------------------
    let tx_stdin = tx.clone();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        loop {
            let mut input = String::new();
            if stdin.read_line(&mut input).is_ok() {
                let trimmed = input.trim().to_string();
                if !trimmed.is_empty() {
                    let _ = tx_stdin.send(AppEvent::Input(trimmed));
                }
            }
        }
    });

    // ------------------------------------------
    // MAIN THREAD: State Machine
    // ------------------------------------------
    let mut state = AppState::MainMenu;
    let mut recent_files: HashMap<String, Instant> = HashMap::new();
    let mut last_cleanup = Instant::now();

    print_main_menu();

    loop {
        // Clear spam memory every 10 seconds
        if last_cleanup.elapsed().as_secs() > 10 {
            recent_files.clear();
            last_cleanup = Instant::now();
        }

        // Determine if we need a timeout loop (for the Challenge Mode timer)
        let timeout = match state {
            AppState::Playing {
                challenge_end: Some(end),
                maps_beaten,
                ..
            } => {
                let now = Instant::now();
                if now >= end {
                    println!("\n⏰ CHALLENGE OVER! ⏰");
                    println!("🏆 Total Maps Beaten: {}\n", maps_beaten);
                    state = AppState::MainMenu;
                    print_main_menu();
                    continue;
                } else {
                    Some(Duration::from_millis(500))
                }
            }
            _ => None,
        };

        // Wait for event or timeout
        let event = if let Some(t) = timeout {
            rx.recv_timeout(t).ok()
        } else {
            rx.recv().ok()
        };

        if event.is_none() {
            continue; // Timeout triggered, loop around to check timer again
        }

        match event.unwrap() {
            // ==========================================
            // KEYBOARD INPUT HANDLING
            // ==========================================
            AppEvent::Input(input) => match state.clone() {
                AppState::MainMenu => match input.as_str() {
                    "1" => {
                        state = AppState::RandomInfiniteMenu;
                        print_random_infinite_menu();
                    }
                    "2" => {
                        state = AppState::ChallengeMenu;
                        print_challenge_menu();
                    }
                    "3" => {
                        state = AppState::ChooseMapInput;
                        println!("\nEnter TM Exchange Map Code:");
                    }
                    _ => println!("Invalid option. Please enter 1, 2, or 3."),
                },
                AppState::RandomInfiniteMenu => match input.as_str() {
                    "1" => start_gameplay(&mut state, PlayMode::BeatAuthor, None, &target_folder),
                    "2" => start_gameplay(&mut state, PlayMode::Grinding, None, &target_folder),
                    "3" => {
                        state = AppState::MainMenu;
                        print_main_menu();
                    }
                    _ => println!("Invalid option. Please enter 1, 2, or 3."),
                },
                AppState::ChallengeMenu => match input.as_str() {
                    "1" => {
                        start_gameplay(&mut state, PlayMode::BeatAuthor, Some(60), &target_folder)
                    }
                    "2" => {
                        start_gameplay(&mut state, PlayMode::BeatAuthor, Some(30), &target_folder)
                    }
                    "3" => {
                        start_gameplay(&mut state, PlayMode::BeatAuthor, Some(15), &target_folder)
                    }
                    "4" => {
                        state = AppState::ChallengeCustomTimeInput;
                        println!("\nEnter whole number of minutes:");
                    }
                    "5" => {
                        state = AppState::MainMenu;
                        print_main_menu();
                    }
                    _ => println!("Invalid option. Please enter 1-5."),
                },
                AppState::ChallengeCustomTimeInput => {
                    if let Ok(mins) = input.parse::<u64>() {
                        start_gameplay(
                            &mut state,
                            PlayMode::BeatAuthor,
                            Some(mins),
                            &target_folder,
                        );
                    } else {
                        println!("Invalid number. Returning to menu...");
                        state = AppState::ChallengeMenu;
                        print_challenge_menu();
                    }
                }
                AppState::ChooseMapInput => {
                    println!("Downloading map {}...", input);
                    match download_specific_map(&input, &target_folder) {
                        Ok(_) => println!("✅ Map loaded! Returning to main menu.\n"),
                        Err(e) => println!("❌ Error: {}\nReturning to main menu.\n", e),
                    }
                    state = AppState::MainMenu;
                    print_main_menu();
                }
                AppState::Playing {
                    current_track_id,
                    last_replay_path,
                    ..
                } => match input.as_str() {
                    "1" => {
                        println!("\n⏭️ Skipping map... Fetching new map...");
                        match download_random_map(&target_folder) {
                            Ok((new_track_id, new_time)) => {
                                if let AppState::Playing {
                                    current_track_id: ref mut tid,
                                    current_author_time: ref mut at,
                                    last_replay_path: ref mut lrp,
                                    maps_beaten,
                                    challenge_end,
                                    ..
                                } = &mut state
                                {
                                    *tid = new_track_id;
                                    *at = new_time;
                                    *lrp = None;
                                    print_playing_instructions(
                                        *at,
                                        *challenge_end,
                                        *maps_beaten,
                                    );
                                }
                            }
                            Err(e) => println!("Failed to skip: {}", e),
                        }
                    }
                    "2" => {
                        println!("\nReturning to Main Menu...\n");
                        state = AppState::MainMenu;
                        print_main_menu();
                    }
                    "9" => {
                        println!("\n🚀 Attempting to upload your last replay...");
                        if let Some(path) = last_replay_path {
                            upload_replay_to_tmx(&path, &current_track_id);
                        } else {
                            println!("❌ No replay found yet! Finish a run to generate an autosave first.");
                        }
                    }
                    _ => println!("Invalid command. Type '1' to Skip, '2' for Main Menu, or '9' to Upload Replay."),
                },
            },

            // ==========================================
            // REPLAY FILE HANDLING
            // ==========================================
            AppEvent::FileActivity(path) => {
                if let AppState::Playing {
                    mode,
                    current_track_id,
                    current_author_time,
                    maps_beaten,
                    challenge_end,
                    last_replay_path,
                } = &mut state
                {
                    // Anti-Spam Check
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
                    recent_files.insert(filename, Instant::now());

                    // SAVE THE NEWEST REPLAY PATH
                    *last_replay_path = Some(path.clone());

                    // Let TM finish writing to disk
                    std::thread::sleep(Duration::from_millis(1000));

                    match get_replay_best_time(&path) {
                        Ok(best_time) => {
                            println!(
                                "   -> Your Time: {} | Target: {}",
                                format_time(best_time),
                                format_time(*current_author_time)
                            );

                            if *mode == PlayMode::BeatAuthor
                                && best_time <= *current_author_time
                                && best_time > 0
                            {
                                *maps_beaten += 1;
                                println!("\n🏆 AUTHOR MEDAL BEAT! (Maps Beaten: {})", maps_beaten);
                                println!("Downloading next map...");

                                match download_random_map(&target_folder) {
                                    Ok((new_track_id, new_time)) => {
                                        *current_track_id = new_track_id;
                                        *current_author_time = new_time;
                                        *last_replay_path = None;
                                        print_playing_instructions(
                                            *current_author_time,
                                            *challenge_end,
                                            *maps_beaten,
                                        );
                                    }
                                    Err(e) => println!("Download failed: {}", e),
                                }
                            } else if *mode == PlayMode::BeatAuthor {
                                println!("❌ Not quite! Keep trying.");
                            } else if *mode == PlayMode::Grinding {
                                println!("🔥 Keep grinding! ('1' skip, '2' menu, '9' upload)");
                            }
                        }
                        Err(e) => eprintln!("Error parsing replay: {}", e),
                    }
                }
            }
        }
    }
}

// ==========================================
// HELPERS & NETWORK FUNCTIONS
// ==========================================

fn start_gameplay(state: &mut AppState, mode: PlayMode, minutes: Option<u64>, target: &Path) {
    println!("\nFetching map...");
    match download_random_map(target) {
        Ok((track_id, time)) => {
            let challenge_end = minutes.map(|m| Instant::now() + Duration::from_secs(m * 60));
            *state = AppState::Playing {
                mode,
                current_track_id: track_id,
                current_author_time: time,
                challenge_end,
                maps_beaten: 0,
                last_replay_path: None,
            };
            print_playing_instructions(time, challenge_end, 0);
        }
        Err(e) => {
            println!("❌ Failed to download map: {}. Returning to menu.", e);
            *state = AppState::MainMenu;
            print_main_menu();
        }
    }
}

fn download_random_map(target_folder: &Path) -> Result<(String, i32), String> {
    let response = reqwest::blocking::get("https://tmnf.exchange/trackrandom")
        .map_err(|e| format!("Network error: {}", e))?;
    let final_url = response.url().as_str();
    let track_id = final_url.split('/').last().unwrap_or("").to_string();

    if track_id.is_empty() {
        return Err("Failed to extract random track ID".into());
    }

    let time = download_specific_map(&track_id, target_folder)?;
    Ok((track_id, time))
}

fn download_specific_map(track_id: &str, target_folder: &Path) -> Result<i32, String> {
    let download_url = format!("https://tmnf.exchange/trackgbx/{}", track_id);
    let response =
        reqwest::blocking::get(&download_url).map_err(|e| format!("Network error: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Map {} not found on TMX (HTTP {})",
            track_id,
            response.status()
        ));
    }

    let bytes = response
        .bytes()
        .map_err(|e| format!("Failed to read bytes: {}", e))?;

    // Extract author time
    let content_str = String::from_utf8_lossy(&bytes);
    let re_author = Regex::new(r#"authortime="(\d+)""#).unwrap();

    let author_time = re_author
        .captures(&content_str)
        .and_then(|cap| cap.get(1))
        .ok_or("Author time not found in map file header")?
        .as_str()
        .parse::<i32>()
        .map_err(|_| "Failed to parse author time")?;

    // Save and open
    let file_path = target_folder.join(format!("map_{}.gbx", track_id));
    let mut file =
        std::fs::File::create(&file_path).map_err(|e| format!("Failed to create file: {}", e))?;
    let mut cursor = Cursor::new(bytes.clone());
    std::io::copy(&mut cursor, &mut file).map_err(|e| format!("Failed to write file: {}", e))?;

    opener::open(&file_path).map_err(|e| format!("Failed to open map in game: {}", e))?;

    Ok(author_time)
}

fn get_replay_best_time(path: &Path) -> Result<i32, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let content = String::from_utf8_lossy(&bytes);

    let re_best = Regex::new(r#"best="(\d+)""#).unwrap();
    let best_time = re_best
        .captures(&content)
        .and_then(|cap| cap.get(1))
        .ok_or("Best time not found in replay file. Game may still be writing.")?
        .as_str()
        .parse::<i32>()
        .map_err(|_| "Failed to parse best time")?;

    Ok(best_time)
}

fn format_time(ms: i32) -> String {
    let seconds = ms / 1000;
    let millis = ms % 1000;
    format!("{}.{:03}s", seconds, millis)
}

fn upload_replay_to_tmx(replay_path: &Path, track_id: &str) {
    println!("🌐 TMX requires session authentication (Ubisoft Connect) to submit records.");
    println!("Opening the TMX map page for you...");
    println!("📁 File ready for upload: {}", replay_path.display());

    let upload_url = format!("https://tmnf.exchange/trackshow/{}", track_id);
    if let Err(e) = opener::open(&upload_url) {
        println!("❌ Failed to open browser: {}", e);
    }

    /* // ==========================================
    // HARDCODED DIRECT UPLOAD (Requires manual cookie extraction)
    // ==========================================
    // If you extract your TMX session cookie from your browser, you can use this snippet
    // to bypass the browser entirely. WARNING: Cookies expire, so this requires maintenance.

    // let cookie = "YOUR_PHPSESSID_OR_TMX_COOKIE_HERE";
    // let upload_endpoint = format!("https://tmnf.exchange/replay/upload/{}", track_id);
    //
    // let form = reqwest::blocking::multipart::Form::new()
    //     .file("replay_file", replay_path)
    //     .expect("Failed to read replay file");
    //
    // let client = reqwest::blocking::Client::new();
    // let res = client.post(&upload_endpoint)
    //     .header(reqwest::header::COOKIE, cookie)
    //     .multipart(form)
    //     .send();
    //
    // match res {
    //     Ok(response) if response.status().is_success() => println!("✅ Upload successful!"),
    //     Ok(response) => println!("⚠️ Upload failed with status: {}", response.status()),
    //     Err(e) => println!("❌ Network error during upload: {}", e),
    // }
    */
}

// ==========================================
// UI MENUS
// ==========================================

fn print_main_menu() {
    println!("\n==================================");
    println!("          MAIN MENU");
    println!("==================================");
    println!("1: Random Infinite");
    println!("2: Random Map Challenge");
    println!("3: Choose map to download");
    println!("----------------------------------");
    println!("Enter selection (1-3):");
}

fn print_random_infinite_menu() {
    println!("\n==================================");
    println!("       RANDOM INFINITE");
    println!("==================================");
    println!("1: Beat Author Time (Auto-skips on win)");
    println!("2: Infinite Grinding (No auto-skips)");
    println!("3: Back to Main Menu");
    println!("----------------------------------");
    println!("Enter selection (1-3):");
}

fn print_challenge_menu() {
    println!("\n==================================");
    println!("      RANDOM MAP CHALLENGE");
    println!("==================================");
    println!("1: 1 Hour");
    println!("2: 30 Minutes");
    println!("3: 15 Minutes");
    println!("4: Custom (Enter minutes)");
    println!("5: Back to Main Menu");
    println!("----------------------------------");
    println!("Select time (1-5):");
}

fn print_playing_instructions(author_time: i32, end_time: Option<Instant>, maps_beaten: u32) {
    println!("\n==================================");
    println!("🎯 TARGET AUTHOR TIME: {}", format_time(author_time));
    if let Some(end) = end_time {
        let remaining = end.duration_since(Instant::now()).as_secs();
        println!(
            "⏱️  TIME REMAINING: {}m {}s",
            remaining / 60,
            remaining % 60
        );
        println!("🏅 Maps Beaten: {}", maps_beaten);
    }
    println!("----------------------------------");
    println!("Type '1' to Skip Map | '2' for Main Menu | '9' to Upload Replay");
    println!("==================================\n");
}
