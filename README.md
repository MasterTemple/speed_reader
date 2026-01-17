# Speed Reader

## Features

- Input via: file, parameter, or stdin
- Adjustable WPM
- Red middle character (allegedly helps with focus)
- Ignores non-words (only comprised of non-alphanumeric characters)

## Controls

- `Space`: Toggle Play/Pause
- `+/-`: Adjust WPM
- `h/←`: Prev Word
- `l/→`: Next Word
- `r`: Restart from beginning
- `z`: Zen (Hide controls and status)
- `q`: Quit

## CLI Usage

```bash
Usage: speed_reader [OPTIONS]

Options:
  -w, --wpm <WPM>    [default: 500]
  -t, --text <TEXT>  
  -f, --file <FILE>  
  -h, --help         Print help
  -V, --version      Print version
```

## Suggestions

- Create an alias to [pick a file](https://github.com/alexpasmantier/television) with your preferred WPM

```bash
alias sr=speed_reader -f "$(tv files)" -wpm 600

```

- Use Zen mode if you zoom in

## Notes

- This had a built-in file picker, but I realized other tools do that better.
- Sorry I don't have a creative name
