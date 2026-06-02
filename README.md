**TrackMania Randomizer Bot**

This bot completely automates a TrackMania "Randomizer" challenge. It gets a random map for you, waits until you beat the Author Time, and then automatically loads the next random map.

**How it Works**

Startup: The bot downloads a random map from TM-Exchange and opens it in your game.

The Goal: It reads the map file to see what the Author Time target is.

Play: You play the map in TrackMania.

Auto-Progress: When you finish, the bot reads your autosaved replay file. If your time beat the Author Time, it instantly downloads and opens the next map.

**Installation**

Go to the Releases section on the right side of this GitHub page.

Download the latest tm_randomizer.exe.

Move the .exe file into an empty folder on your desktop (this is where your downloaded maps will save).

Double-click tm_randomizer.exe to run it.

**Important Note**: The bot assumes your TrackMania game is installed in the default Windows location (C:\Users\<YourName>\Documents\TmForever). If you use OneDrive or installed the game to a custom drive, you will need to compile the source code manually and adjust the file paths in main.rs.

**How to Use It**

Run the tm_randomizer.exe file.

TrackMania will automatically open a random map. Start playing!

If you win: The console will say 🏆 Author Medal beat! and automatically launch your next track.

If you get stuck: Go to the black console window, type s, and press Enter to skip the map and get a new one.
