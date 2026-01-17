use anyhow::{Context, Result};
use clap::Parser;
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    style::{Color, Print, SetForegroundColor},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use ignore::WalkBuilder;
use std::io::{self, Read, Write};
use std::process;
use std::time::{Duration, Instant};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value_t = 500)]
    wpm: u32,

    #[arg(short, long)]
    text: Option<String>,

    #[arg(short, long, name = "FILE")]
    file: Option<String>,
}

struct SpeedReader {
    words: Vec<String>,
    current_word_index: usize,
    wpm: u32,
    is_paused: bool,
}

pub struct FormatDuration(Duration);
impl std::fmt::Display for FormatDuration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let total_seconds = self.0.as_secs();
        let minutes = total_seconds / 60;
        let seconds = total_seconds % 60;
        let millis = self.0.subsec_millis();
        write!(f, "{:02}:{:02}.{:03}", minutes, seconds, millis)
    }
}

impl SpeedReader {
    fn new(words: Vec<String>, wpm: u32) -> Self {
        Self {
            words,
            current_word_index: 0,
            wpm,
            is_paused: true,
        }
    }

    fn current_word(&self) -> Option<&str> {
        self.words.get(self.current_word_index).map(|s| s.as_str())
    }

    fn next_word(&mut self) -> bool {
        if self.current_word_index < self.words.len() - 1 {
            self.current_word_index += 1;
            true
        } else {
            false
        }
    }

    fn previous_word(&mut self) -> bool {
        if self.current_word_index > 0 {
            self.current_word_index -= 1;
            true
        } else {
            false
        }
    }

    fn restart(&mut self) {
        self.current_word_index = 0;
        self.is_paused = true;
    }

    fn adjust_wpm(&mut self, delta: i32) {
        let new_wpm = self.wpm as i32 + delta;
        if new_wpm >= 50 && new_wpm <= 5000 {
            self.wpm = new_wpm as u32;
        }
    }

    fn get_display_interval(&self) -> Duration {
        Duration::from_secs_f64(60.0 / self.wpm as f64)
    }

    fn start_reading(&mut self) {
        self.is_paused = false;
    }

    fn pause_reading(&mut self) {
        self.is_paused = true;
    }

    fn render(&self) -> Result<()> {
        let (width, height) = terminal::size()?;
        let word = self.current_word().unwrap_or("");
        let word_len = word.len();

        let pivot_index = word_len / 2;

        let row = height / 2;
        let col = width / 2 - (word_len as u16 / 2);

        execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, row),)?;

        for (i, c) in word.chars().enumerate() {
            execute!(
                io::stdout(),
                MoveTo(col + i as u16, row),
                if i == pivot_index {
                    SetForegroundColor(Color::Red)
                } else {
                    SetForegroundColor(Color::White)
                },
                Print(c),
            )?;
        }

        self.render_controls(width, height)?;

        io::stdout().flush()?;
        Ok(())
    }

    fn render_controls(&self, width: u16, height: u16) -> Result<()> {
        let controls = [
            ("[Space]", "Play/Pause"),
            ("[+/-]", "WPM"),
            ("[↑/←]", "Prev"),
            ("[↓/→]", "Next"),
            ("[r]", "Restart"),
            ("[o]", "Open"),
            ("[q]", "Quit"),
        ];

        let row = height - 2;
        let controls_width: usize = controls
            .iter()
            .map(|(k, a)| k.len() + a.len() + 3)
            .sum::<usize>()
            - 1;
        let col = width / 2 - (controls_width as u16 / 2);

        execute!(
            io::stdout(),
            MoveTo(col, row),
            SetForegroundColor(Color::DarkGrey),
        )?;

        for (i, (key, action)) in controls.iter().enumerate() {
            if i > 0 {
                execute!(io::stdout(), Print("  "))?;
            }
            execute!(
                io::stdout(),
                SetForegroundColor(Color::Cyan),
                Print(key),
                SetForegroundColor(Color::DarkGrey),
                Print(format!(" {} ", action)),
            )?;
        }

        let status_row = row - 1;

        let status_text = format!(
            "{} | Word {}/{} | WPM: {} | Percent: {:.0}% | Remaining: {}",
            if self.is_paused { "PAUSED" } else { "PLAYING" },
            self.current_word_index + 1,
            self.words.len(),
            self.wpm,
            ((self.current_word_index as f64 + 1.0) / self.words.len() as f64) * 100.0,
            FormatDuration(Duration::from_secs_f64(
                (self.words.len() - self.current_word_index) as f64 / (self.wpm as f64 / 60.0)
            ))
        );

        let status_col = width / 2 - (status_text.len() as u16 / 2);
        execute!(
            io::stdout(),
            MoveTo(status_col, status_row),
            SetForegroundColor(Color::Yellow),
            Print(status_text),
        )?;

        Ok(())
    }

    fn run(&mut self) -> Result<()> {
        if self.words.is_empty() {
            eprintln!("No words to display");
            return Ok(());
        }

        self.render()?;

        let mut last_update = Instant::now();
        // Cache this value
        let mut display_interval = self.get_display_interval();

        loop {
            if !self.is_paused {
                let now = Instant::now();
                if now.duration_since(last_update) >= display_interval {
                    self.render()?;
                    self.next_word();
                    last_update = now;
                }

                if self.current_word_index >= self.words.len() {
                    break;
                }
            }

            if event::poll(Duration::from_millis(50))? {
                // The event must be shared, or else I miss every other keystroke
                let ev = event::read()?;
                if let Event::Key(KeyEvent { code, .. }) = ev {
                    match code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            return Ok(());
                        }
                        KeyCode::Char(' ') => {
                            if self.is_paused {
                                self.start_reading();
                            } else {
                                self.pause_reading();
                            }
                            last_update = Instant::now();
                            self.render()?;
                        }
                        KeyCode::Char('r') => {
                            self.restart();
                            self.render()?;
                        }
                        KeyCode::Char('l') | KeyCode::Right | KeyCode::Down => {
                            if self.next_word() {
                                self.render()?;
                                last_update = Instant::now();
                            } else if self.current_word_index >= self.words.len() {
                                break;
                            }
                        }
                        KeyCode::Char('h') | KeyCode::Left | KeyCode::Up => {
                            if self.previous_word() {
                                self.render()?;
                                last_update = Instant::now();
                            }
                        }
                        KeyCode::Char('+') | KeyCode::Char('=') => {
                            self.adjust_wpm(50);
                            display_interval = self.get_display_interval();
                            self.render()?;
                        }
                        KeyCode::Char('-') => {
                            self.adjust_wpm(-50);
                            display_interval = self.get_display_interval();
                            self.render()?;
                        }
                        KeyCode::Char('o') => {
                            if let Some(new_words) = self.open_file_picker()? {
                                self.words = new_words;
                                self.restart();
                                self.render()?;
                            } else {
                                self.render()?;
                            }
                        }
                        _ => {}
                    }
                }

                if let Event::Resize(_, _) = ev {
                    self.render()?;
                }
            }
        }

        Ok(())
    }

    fn open_file_picker(&self) -> Result<Option<Vec<String>>> {
        let mut filter = String::new();
        let mut files: Vec<String> = Vec::new();
        let mut selected_index: usize = 0;

        loop {
            let (width, height) = terminal::size()?;
            execute!(io::stdout(), Clear(ClearType::All))?;

            if filter.is_empty() {
                files = self.get_text_files()?;
            } else {
                files = self.filter_text_files(&filter)?;
            }

            if files.is_empty() {
                execute!(
                    io::stdout(),
                    MoveTo(2, 1),
                    SetForegroundColor(Color::Yellow),
                    Print("No matching text files found"),
                    MoveTo(2, 3),
                    SetForegroundColor(Color::Cyan),
                    Print("Type to filter files: "),
                    SetForegroundColor(Color::White),
                    Print(&filter),
                    Print("_"),
                )?;
            } else {
                execute!(
                    io::stdout(),
                    MoveTo(2, 1),
                    SetForegroundColor(Color::Yellow),
                    Print("File Picker - Select a text file to open:"),
                    MoveTo(2, 3),
                    SetForegroundColor(Color::Cyan),
                    Print("Type to filter files: "),
                    SetForegroundColor(Color::White),
                    Print(&filter),
                    Print("_"),
                )?;

                let max_display = (height - 6) as usize;
                let start: usize = selected_index.saturating_sub(max_display / 2);
                let end = std::cmp::min(start + max_display, files.len());

                for (i, file) in files.iter().enumerate().take(end).skip(start) {
                    let row = 5 + (i - start) as u16;
                    let filename = file.split('/').last().unwrap_or(file);

                    let display_name = if filename.len() > (width - 10) as usize {
                        format!("...{}", &filename[filename.len() - (width - 13) as usize..])
                    } else {
                        filename.to_string()
                    };

                    execute!(
                        io::stdout(),
                        MoveTo(4, row),
                        SetForegroundColor(if i == selected_index {
                            Color::Red
                        } else {
                            Color::White
                        }),
                        if i == selected_index {
                            Print("► ")
                        } else {
                            Print("  ")
                        },
                        Print(display_name),
                    )?;
                }
            }

            io::stdout().flush()?;

            if let Event::Key(KeyEvent {
                code, modifiers, ..
            }) = event::read()?
            {
                match code {
                    KeyCode::Esc => return Ok(None),
                    KeyCode::Enter => {
                        if let Some(file) = files.get(selected_index) {
                            let text = read_file(file)?;
                            if !text.trim().is_empty() {
                                return Ok(Some(parse_words(&text)));
                            }
                        }
                    }
                    KeyCode::Up => {
                        if selected_index > 0 {
                            selected_index -= 1;
                        }
                    }
                    KeyCode::Char('p') if modifiers.contains(KeyModifiers::CONTROL) => {
                        if selected_index > 0 {
                            selected_index -= 1;
                        }
                    }
                    KeyCode::Char('n') if modifiers.contains(KeyModifiers::CONTROL) => {
                        if selected_index < files.len().saturating_sub(1) {
                            selected_index += 1;
                        }
                    }
                    KeyCode::Down => {
                        if selected_index < files.len().saturating_sub(1) {
                            selected_index += 1;
                        }
                    }
                    KeyCode::Char(c) if c.is_ascii() && !c.is_ascii_control() => {
                        filter.push(c);
                        selected_index = 0;
                    }
                    KeyCode::Backspace => {
                        filter.pop();
                        selected_index = 0;
                    }
                    _ => {}
                }
            }

            // if event::poll(Duration::from_millis(10))? {
            //     if let Event::Resize(_, _) = event::read()? {}
            // }
        }
    }

    fn get_text_files(&self) -> Result<Vec<String>> {
        let mut files = Vec::new();

        let walk = WalkBuilder::new(".")
            .hidden(false)
            .git_ignore(false)
            .parents(true)
            .build();

        for result in walk {
            if let Ok(entry) = result {
                let path = entry.path();
                if path.is_file() {
                    if let Some(ext) = path.extension() {
                        let ext_lower = ext.to_string_lossy().to_lowercase();
                        if matches!(ext_lower.as_str(), "txt" | "md" | "rst" | "log" | "text") {
                            files.push(path.to_string_lossy().to_string());
                        }
                    }
                }
            }
        }

        files.sort();
        Ok(files)
    }

    fn filter_text_files(&self, filter: &str) -> Result<Vec<String>> {
        let all_files = self.get_text_files()?;
        let filter_lower = filter.to_lowercase();

        let filtered: Vec<String> = all_files
            .into_iter()
            .filter(|path| path.to_lowercase().contains(&filter_lower))
            .collect();

        Ok(filtered)
    }
}

fn read_stdin() -> Result<String> {
    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer)?;
    Ok(buffer)
}

fn read_file(path: &str) -> Result<String> {
    std::fs::read_to_string(path).with_context(|| format!("Failed to read file: {}", path))
}

fn parse_words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .filter(|s| s.contains(char::is_alphanumeric))
        .map(|s| s.to_string())
        .collect()
}

fn main() -> Result<()> {
    let args = Args::parse();

    let text = if let Some(text) = args.text {
        text
    } else if let Some(file) = args.file {
        read_file(&file)?
    } else {
        read_stdin()?
    };

    let words = parse_words(&text);

    if words.is_empty() {
        eprintln!("No words found in input");
        process::exit(1);
    }

    terminal::enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen, Hide)?;

    let mut reader = SpeedReader::new(words, args.wpm);

    let result = reader.run();

    execute!(io::stdout(), LeaveAlternateScreen, Show)?;
    terminal::disable_raw_mode()?;

    result?;

    Ok(())
}
