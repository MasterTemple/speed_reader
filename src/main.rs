use anyhow::{Context, Result};
use clap::Parser;
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEvent},
    execute,
    style::{Color, Print, SetForegroundColor},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io::{self, Read, Write};
use std::process;
use std::time::{Duration, Instant};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    /// WPM
    #[arg(short, long, default_value_t = 500)]
    wpm: u32,

    /// Word Index
    #[arg(short, long, default_value_t = 0)]
    index: usize,

    /// Text Input
    #[arg(short, long)]
    text: Option<String>,

    /// File Input
    #[arg(short, long, name = "FILE")]
    file: Option<String>,
}

struct SpeedReader {
    words: Vec<String>,
    current_word_index: usize,
    wpm: u32,
    is_paused: bool,
    hide_bar: bool,
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
    fn new(words: Vec<String>, wpm: u32, index: usize) -> Self {
        Self {
            words,
            current_word_index: index,
            wpm,
            is_paused: true,
            hide_bar: false,
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

        if !self.hide_bar {
            self.render_controls(width, height)?;
        }

        io::stdout().flush()?;
        Ok(())
    }

    fn render_controls(&self, width: u16, height: u16) -> Result<()> {
        let controls = [
            ("[Space]", "Play/Pause"),
            ("[+/-]", "WPM"),
            ("[h/←]", "Prev"),
            ("[l/→]", "Next"),
            ("[r]", "Restart"),
            ("[z]", "Zen"),
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
            "{} | Word {}/{} ({:.0}%) | WPM: {} | Remaining: {}",
            if self.is_paused { "▶" } else { "⏸" },
            self.current_word_index + 1,
            self.words.len(),
            ((self.current_word_index as f64 + 1.0) / self.words.len() as f64) * 100.0,
            self.wpm,
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
                        KeyCode::Char('l') | KeyCode::Right => {
                            if self.next_word() {
                                self.render()?;
                                last_update = Instant::now();
                            } else if self.current_word_index >= self.words.len() {
                                break;
                            }
                        }
                        KeyCode::Char('h') | KeyCode::Left => {
                            if self.previous_word() {
                                self.render()?;
                                last_update = Instant::now();
                            }
                        }
                        KeyCode::Char('z') => {
                            self.hide_bar = !self.hide_bar;
                            self.render()?;
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

    let mut reader = SpeedReader::new(words, args.wpm, args.index);

    let result = reader.run();

    execute!(io::stdout(), LeaveAlternateScreen, Show)?;
    terminal::disable_raw_mode()?;

    result?;

    Ok(())
}
