use blame_finder::{Repository, cleanup_old_repos, find_oldest_todo};
use std::env;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Get repository URL from command-line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <repository-url>", args[0]);
        std::process::exit(1);
    }

    let repo_url = &args[1];
    println!("Searching for TODOs in {}", repo_url);

    // Find the oldest TODO
    let repository = Repository::new(repo_url).await.unwrap();
    match find_oldest_todo(&repository).await {
        Ok(Some(todo)) => {
            println!("\nFound oldest TODO!");
            println!("File: {}", todo.file_path);
            println!("Line: {}", todo.line_number);
            println!("Text: {}", todo.todo_text);
            println!("\nContext:");
            println!("{}", todo.context_code);

            if let Some(blame) = todo.blame_info {
                println!("\nAuthor: {} <{}>", blame.author, blame.author_email);
                println!("Date: {}", blame.date.format("%Y-%m-%d %H:%M:%S"));
                println!("Commit: {}", blame.commit_hash);
                println!("Message: {}", blame.summary);
            }
        }
        Ok(None) => {
            println!("No TODOs found in the repository.");
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }

    // Clean up old repos (older than 7 days)
    if let Ok(count) = cleanup_old_repos(7, None).await {
        if count > 0 {
            println!("\nCleaned up {} old repositories.", count);
        }
    }

    Ok(())
}
