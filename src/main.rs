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
use std::time::Duration;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value_t = 300)]
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

impl SpeedReader {
    fn new(words: Vec<String>, wpm: u32) -> Self {
        Self {
            words,
            current_word_index: 0,
            wpm,
            is_paused: false,
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
    }

    fn get_display_interval(&self) -> Duration {
        Duration::from_secs_f64(60.0 / self.wpm as f64)
    }

    fn format_word(&self, word: &str) -> String {
        if word.is_empty() {
            return word.to_string();
        }

        let word_len = word.len();
        let pivot_index = if word_len <= 4 {
            word_len / 2
        } else {
            word_len / 3 + 1
        };

        let mut result = String::new();
        for (i, c) in word.chars().enumerate() {
            if i == pivot_index {
                result.push(c);
            } else {
                result.push(c);
            }
        }
        result
    }

    fn render(&self) -> Result<()> {
        let (width, height) = terminal::size()?;
        let word = self.current_word().unwrap_or("");
        let word_len = word.len();

        let pivot_index = if word_len <= 4 {
            word_len / 2
        } else {
            word_len / 3 + 1
        };

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
            ("[↑/←]", "Previous"),
            ("[↓/→]", "Next"),
            ("[r]", "Restart"),
            ("[q]", "Quit"),
        ];

        let row = height - 2;
        let col = width / 2 - (controls.iter().map(|(_, d)| d.len() + 2).sum::<usize>() as u16 / 2);

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
            "{} | Word {}/{} | WPM: {}{}",
            if self.is_paused { "PAUSED" } else { "PLAYING" },
            self.current_word_index + 1,
            self.words.len(),
            self.wpm,
            if self.is_paused {
                " - Press Space to resume"
            } else {
                ""
            },
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

        let mut last_update = std::time::Instant::now();

        loop {
            if !self.is_paused {
                let now = std::time::Instant::now();
                if now.duration_since(last_update) >= self.get_display_interval() {
                    self.render()?;
                    self.next_word();
                    last_update = now;
                }

                if self.current_word_index >= self.words.len() {
                    break;
                }
            }

            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(KeyEvent { code, .. }) = event::read()? {
                    match code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            return Ok(());
                        }
                        KeyCode::Char(' ') => {
                            self.is_paused = !self.is_paused;
                            if !self.is_paused {
                                last_update = std::time::Instant::now();
                            }
                            self.render()?;
                        }
                        KeyCode::Char('r') => {
                            self.restart();
                            self.is_paused = false;
                            last_update = std::time::Instant::now();
                            self.render()?;
                        }
                        KeyCode::Right | KeyCode::Down => {
                            if self.next_word() {
                                self.render()?;
                                last_update = std::time::Instant::now();
                            } else if self.current_word_index >= self.words.len() {
                                break;
                            }
                        }
                        KeyCode::Left | KeyCode::Up => {
                            if self.previous_word() {
                                self.render()?;
                                last_update = std::time::Instant::now();
                            }
                        }
                        _ => {}
                    }
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
    text.split_whitespace().map(|s| s.to_string()).collect()
}

fn main() -> Result<()> {
    let args = Args::parse();

    let text = if args.text.is_some() {
        args.text.unwrap()
    } else if args.file.is_some() {
        read_file(&args.file.unwrap())?
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
