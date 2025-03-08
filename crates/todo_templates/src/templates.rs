use askama::Template;
use blame_finder::{BlameInfo, TodoItem};

// Specific content templates
#[derive(Template)]
#[template(path = "index_content.html")]
pub struct IndexContent;

#[derive(Template)]
#[template(path = "result_content.html")]
pub struct ResultContent {
    pub result: TodoItem,
}

#[derive(Template)]
#[template(path = "error_content.html")]
pub struct ErrorContent<'a> {
    pub error: &'a str,
}

#[derive(Debug, Clone, PartialEq)]
// TODO: make these references, not owned
pub struct TodoItemDisplay {
    /// Relative path to the file containing the TODO
    pub file_path: String,

    /// Line number where the TODO appears
    pub line_number: u32,

    /// The actual TODO text
    pub todo_text: String,

    /// Surrounding code context
    pub context_code: String,

    /// Information about the commit that introduced this TODO
    pub blame_info: BlameInfo,

    /// The source repo url, copied here for easy displaying
    pub source_repo_url: String,

    pub permalink_url: String,

    pub display_repo_name: String,
}

impl From<TodoItem> for TodoItemDisplay {
    fn from(value: TodoItem) -> Self {
        TodoItemDisplay {
            file_path: value.file_path.clone(),
            line_number: value.line_number.clone(),
            todo_text: value.todo_text.clone(),
            context_code: value.context_code.clone(),
            blame_info: value
                .blame_info
                .clone()
                .expect("Should never try and display todo info without blame info"),
            permalink_url: value.get_permalink_url(),
            display_repo_name: value.get_repo_display_name(),
            source_repo_url: value.source_repo_url,
        }
    }
}

#[derive(Template)]
#[template(path = "leaderboard_content.html")]
pub struct LeaderboardTemplate {
    pub todos: Vec<TodoItemDisplay>,
    pub todos_length: usize,
}
