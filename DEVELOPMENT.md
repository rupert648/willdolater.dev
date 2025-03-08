# Development Guide

This document provides guidance for developers working on the Oldest TODO Finder project.

## Development Environment

### Prerequisites

1. Rust (latest stable version)
2. Git
3. ripgrep
4. An IDE with Rust support (VS Code with rust-analyzer recommended)

### Setting Up for Development

1. **Clone the repository**
   ```bash
   git clone https://github.com/yourusername/oldest-todo-finder.git
   cd oldest-todo-finder
   ```

2. **Install dependencies**
   Make sure you have Git and ripgrep installed on your system.

3. **Build the project in development mode**
   ```bash
   cargo build
   ```

4. **Run tests**
   ```bash
   cargo test --workspace
   ```

## Project Structure

```
.
├── blame_finder/             # Library crate for git/todo functionality
│   ├── src/
│   │   ├── lib.rs            # Main library entry points
│   │   ├── repo.rs           # Repository management
│   │   ├── todo.rs           # TODO finding logic
│   │   ├── blame.rs          # Git blame analysis
│   │   └── error.rs          # Error definitions
│   ├── examples/             # Example CLI applications
│   └── Cargo.toml            # Library dependencies
├── src/                      # Web server application
│   ├── main.rs               # Server entry point
│   └── templates.rs          # HTML templating
├── static/                   # Static assets
│   └── css/
│       └── styles.css        # Styling for the web UI
└── Cargo.toml                # Main application dependencies
```

## Key Components

### blame_finder Library

- **Repository Module**: Handles cloning and updating Git repositories
- **Todo Module**: Uses ripgrep to find TODO comments in repositories
- **Blame Module**: Uses Git blame to determine when TODOs were added
- **Error Module**: Error types for the library

### Web Server

- **Main**: Sets up the Axum server and routes
- **Templates**: Generates HTML responses for the web UI

## Adding New Features

### Adding Support for More Comment Types

To add support for additional comment types (like "FIXME" or "XXX"):

1. Modify `blame_finder/src/todo.rs` to search for additional patterns:
   ```rust
   let output = Command::new("rg")
       .current_dir(repo.path())
       .arg("TODO|FIXME|XXX")  // Add additional patterns here
       .arg("--line-number")
       // ...
   ```

2. Update the UI to reflect these changes in `src/templates.rs`.

### Supporting Different Repository Hosts

The current implementation primarily targets GitHub, but can be extended:

1. Update URL parsing in `blame_finder/src/repo.rs` to support more hosts:
   ```rust
   if !["github.com", "gitlab.com", "bitbucket.org", "your-new-host.com"].contains(&host) {
       // ...
   }
   ```

## Troubleshooting

### Common Issues

1. **"Command not found" errors**:
   - Ensure `git` and `rg` (ripgrep) are installed and in your PATH

2. **Permission Issues**:
   - Check that your application has permission to create directories in the user's home folder
   
3. **ripgrep Pattern Issues**:
   - Try running the ripgrep command manually to debug pattern matching problems

### Debug Logging

The application uses `tracing` for logging. You can set the log level using the `RUST_LOG` environment variable:

```bash
RUST_LOG=debug cargo run
```

## Performance Optimization

If you need to improve performance:

1. **Limit Repository Depth**:
   - Adjust the git clone depth in `repo.rs` to balance history availability with performance

2. **Add More File Filters**:
   - Modify the ripgrep file filters to focus on relevant file types

3. **Add Caching**:
   - Implement a caching layer for repository results

## Deployment

For deploying to production:

1. **Build the release version**:
   ```bash
   cargo build --release
   ```

2. **Setup the static directory**:
   Ensure the `static` directory is accessible to the running application.

3. **Configure port**:
   The application reads the `PORT` environment variable (default: 3000).

4. **Setup automatic cleanup**:
   The application handles cleanup itself, but you might want to add additional system-level cleanup as a fallback.
