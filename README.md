# Oldest TODO Finder

A high-performance Rust application that finds the oldest TODO comment in any Git repository, using the exact Git blame information to identify when a TODO was added and by whom.

## Features

- **Fast Repository Scanning**: Uses ripgrep for lightning-fast code searching
- **Accurate Blame Information**: Leverages Git blame to find the exact commit that introduced each TODO
- **Web Interface**: Easy-to-use web UI for submitting repositories
- **Flashy Results**: The author's name flashes dramatically as requested!
- **Background Cleanup**: Automatically removes old repository clones to save disk space

## Requirements

- Rust (latest stable)
- Git (command-line)
- ripgrep (`rg` command-line tool)

## Project Structure

The project is organized as a workspace with one crates:

* **blame_finder**: A library that handles repository cloning, TODO finding, and Git blame analysis

And a binary for running the axum server:
* **oldest-todo-finder**: The Axum web server that provides the user interface

## Setup and Development

### 1. Install Dependencies

First, make sure you have Git and ripgrep installed:

```bash
# Ubuntu/Debian
sudo apt install git ripgrep

# macOS
brew install git ripgrep

# Windows (with Chocolatey)
choco install git ripgrep
```

### 2. Clone and Build

```bash
# Clone the repository
git clone https://github.com/yourusername/oldest-todo-finder.git
cd oldest-todo-finder

# Build the project
cargo build --release
```

### 3. Run the Server

```bash
# Run the server
cargo run --release
```

By default, the server will listen on `http://localhost:3000`.

### 4. Run the Example CLI Tool

```bash
# Find the oldest TODO in a repository
cargo run --example find_todos -- https://github.com/username/repo
```

## Library API

The `blame_finder` crate provides a simple API:

```rust
// Find the oldest TODO in a repository
let result = blame_finder::find_oldest_todo("https://github.com/username/repo").await?;

// Clean up old repository clones (older than 7 days)
let cleaned_count = blame_finder::cleanup_old_repos(7).await?;
```

## How It Works

1. The repository is cloned to a local directory (or updated if it already exists)
2. ripgrep searches for "TODO" comments across all code files
3. For each TODO, git blame determines who added it and when
4. The TODOs are sorted by date to find the oldest one
5. The results are displayed with the author's name flashing dramatically

## Configuration

The application creates a `.oldest-todo-finder` directory in your home folder to store cloned repositories. These are automatically cleaned up after 7 days of inactivity.

## Performance

This implementation is designed for speed:

- ripgrep is used for fast code searching (can be 10x faster than alternatives)
- Git operations use efficient commands
- Cloned repositories are cached to avoid repeated cloning

## License

MIT
