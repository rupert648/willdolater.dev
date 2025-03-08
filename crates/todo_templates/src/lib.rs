pub mod templates;

pub use templates::{ErrorContent, IndexContent, LeaderboardTemplate, ResultContent};

pub fn render_template<T: askama::Template>(template: T) -> String {
    match template.render() {
        Ok(html) => html,
        Err(err) => {
            eprintln!("Template rendering error: {}", err);
            format!(
                "<html><body><h1>Internal Error</h1><p>Failed to render template</p></body></html>"
            )
        }
    }
}
