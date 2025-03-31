use axum::response::Html;
use blame_finder::TodoItem;
use todo_templates::{
    ErrorContent, IndexContent, LeaderboardTemplate, ResultContent, render_template,
};

// Public handler functions
pub fn index_page() -> Html<String> {
    Html(render_template(IndexContent))
}

pub fn result_page(todo_item: TodoItem) -> Html<String> {
    Html(render_template(ResultContent { result: todo_item }))
}

pub fn error_page(error_message: &str) -> Html<String> {
    Html(render_template(ErrorContent {
        error: error_message,
    }))
}

pub fn leaderboard_page(todos: Vec<TodoItem>) -> Html<String> {
    let todos_length = todos.len();
    Html(render_template(LeaderboardTemplate {
        todos: todos.into_iter().map(|item| item.into()).collect(),
        todos_length,
    }))
}
