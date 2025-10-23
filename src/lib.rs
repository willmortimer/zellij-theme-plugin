mod data;

use data::ThemeData;
use std::collections::BTreeMap;
use std::io::{self, Write};
use tokio::runtime::Runtime;
use zellij_tile::prelude::*;

register_plugin!(ThemeSelectorPlugin);

struct ThemeSelectorPlugin {
    app: Option<App>,
    theme_data: Option<ThemeData>,
    status_message: String,
    permission_requested: bool,
    permissions_granted: bool,
    last_error: Option<String>,
}

impl ThemeSelectorPlugin {
    fn new_with_status() -> Self {
        ThemeSelectorPlugin {
            app: None,
            theme_data: None,
            status_message: "Requesting permissions...".to_string(),
            permission_requested: false,
            permissions_granted: false,
            last_error: None,
        }
    }

    fn initialize_theme_data(&mut self) -> bool {
        match ThemeData::new() {
            Ok(theme_data) => {
                if let Err(err) = theme_data.ensure_theme_dir() {
                    self.last_error = Some(format!("Error checking theme directory: {err}"));
                    self.status_message = "Failed to verify theme directory".to_string();
                    return true;
                }
                self.theme_data = Some(theme_data);
                self.status_message = "Loading themes...".to_string();
                self.fetch_themes(false)
            }
            Err(err) => {
                self.last_error = Some(format!("Error initializing theme data: {err}"));
                self.status_message = "Failed to initialize theme data".to_string();
                true
            }
        }
    }

    fn fetch_themes(&mut self, force_refresh: bool) -> bool {
        if self.theme_data.is_none() {
            self.status_message = "Theme data not ready".to_string();
            return true;
        }

        let runtime = match Runtime::new() {
            Ok(runtime) => runtime,
            Err(err) => {
                self.last_error = Some(format!("Failed to create Tokio runtime: {err}"));
                self.status_message = "Internal error creating runtime".to_string();
                return true;
            }
        };

        let previous_selection = self
            .app
            .as_ref()
            .and_then(|app| app.selected_theme().map(|theme| theme.to_string()));

        match runtime.block_on(ThemeData::fetch_themes(force_refresh)) {
            Ok(themes) => {
                if themes.is_empty() {
                    self.status_message = "No themes found".to_string();
                } else {
                    self.status_message =
                        "Press Enter to apply theme, j/k or arrows to navigate".to_string();
                }
                let mut app = App::new(themes);
                if let Some(previous) = previous_selection {
                    app.select_theme_by_name(&previous);
                }
                self.app = Some(app);
                self.last_error = None;
            }
            Err(err) => {
                self.last_error = Some(format!("Error fetching themes: {err}"));
                self.status_message = "Failed to fetch themes".to_string();
            }
        }

        true
    }

    fn handle_key_event(&mut self, key: KeyWithModifier) -> bool {
        let mut should_render = false;
        match key.bare_key {
            BareKey::Char('q') if key.key_modifiers.is_empty() => {
                close_self();
            }
            BareKey::Char('r') if key.key_modifiers.is_empty() => {
                if self.permissions_granted {
                    self.status_message = "Refreshing themes...".to_string();
                    should_render = self.fetch_themes(true);
                }
            }
            BareKey::Enter if key.key_modifiers.is_empty() => {
                if let (Some(app), Some(theme_data)) = (self.app.as_ref(), self.theme_data.as_ref())
                {
                    if let Some(selected_theme) = app.selected_theme() {
                        match theme_data.update_config(selected_theme) {
                            Ok(_) => {
                                self.status_message =
                                    format!("Successfully applied theme: {selected_theme}");
                                self.last_error = None;
                            }
                            Err(err) => {
                                self.last_error = Some(format!("Error updating config: {err}"));
                                self.status_message =
                                    format!("Failed to apply theme: {selected_theme}");
                            }
                        }
                        should_render = true;
                    }
                }
            }
            BareKey::Down | BareKey::Char('j') => {
                if let Some(app) = self.app.as_mut() {
                    app.next();
                    should_render = true;
                }
            }
            BareKey::Up | BareKey::Char('k') => {
                if let Some(app) = self.app.as_mut() {
                    app.previous();
                    should_render = true;
                }
            }
            _ => {}
        }

        should_render
    }
}

impl Default for ThemeSelectorPlugin {
    fn default() -> Self {
        ThemeSelectorPlugin::new_with_status()
    }
}

impl ZellijPlugin for ThemeSelectorPlugin {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        if !self.permission_requested {
            set_selectable(true);
            subscribe(&[
                EventType::Key,
                EventType::PermissionRequestResult,
                EventType::Visible,
            ]);
            request_permission(&[PermissionType::WebAccess, PermissionType::Reconfigure]);
            self.permission_requested = true;
        }
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::PermissionRequestResult(status) => match status {
                PermissionStatus::Granted => {
                    self.permissions_granted = true;
                    return self.initialize_theme_data();
                }
                PermissionStatus::Denied => {
                    self.permissions_granted = false;
                    self.status_message = "Required permissions denied".to_string();
                    self.last_error =
                        Some("The plugin needs web and configuration permissions".to_string());
                    return true;
                }
            },
            Event::Key(key) => {
                return self.handle_key_event(key);
            }
            Event::Visible(true) => {
                return true;
            }
            _ => {}
        }

        false
    }

    fn render(&mut self, rows: usize, cols: usize) {
        let mut lines = Vec::new();
        lines.push(self.format_line(&self.status_message, cols));

        if let Some(error) = &self.last_error {
            lines.push(self.format_line(&format!("Last error: {error}"), cols));
        }

        if let Some(app) = self.app.as_mut() {
            if rows > lines.len() {
                let available_rows = rows - lines.len();
                lines.extend(app.render_lines(available_rows, cols));
            }
        } else if rows > lines.len() {
            lines.push(self.format_line("Waiting for theme data...", cols));
        }

        while lines.len() < rows {
            lines.push(self.format_line("", cols));
        }

        print!("\u{1b}[2J\u{1b}[H");
        for line in lines.into_iter().take(rows) {
            println!("{line}");
        }
        io::stdout().flush().ok();
    }
}

struct App {
    themes: Vec<String>,
    selected: usize,
    scroll_offset: usize,
}

impl App {
    fn new(themes: Vec<String>) -> Self {
        App {
            themes,
            selected: 0,
            scroll_offset: 0,
        }
    }

    fn selected_theme(&self) -> Option<&str> {
        self.themes.get(self.selected).map(|theme| theme.as_str())
    }

    fn select_theme_by_name(&mut self, name: &str) {
        if let Some(position) = self.themes.iter().position(|theme| theme == name) {
            self.selected = position;
        } else if self.selected >= self.themes.len() {
            self.selected = self.themes.len().saturating_sub(1);
        }
        self.scroll_offset = 0;
    }

    fn next(&mut self) {
        if self.themes.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.themes.len();
    }

    fn previous(&mut self) {
        if self.themes.is_empty() {
            return;
        }
        if self.selected == 0 {
            self.selected = self.themes.len() - 1;
        } else {
            self.selected -= 1;
        }
    }

    fn render_lines(&mut self, max_rows: usize, cols: usize) -> Vec<String> {
        if self.themes.is_empty() {
            return vec![format_line_with_width("No themes available", cols)];
        }

        let capacity = max_rows.max(1);
        if self.selected >= self.themes.len() {
            self.selected = self.themes.len().saturating_sub(1);
        }
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        } else if self.selected >= self.scroll_offset + capacity {
            self.scroll_offset = self.selected + 1 - capacity;
        }
        let max_offset = self.themes.len().saturating_sub(capacity);
        if self.scroll_offset > max_offset {
            self.scroll_offset = max_offset;
        }

        let end = (self.scroll_offset + capacity).min(self.themes.len());

        let mut rendered = Vec::new();
        for (index, theme) in self.themes[self.scroll_offset..end].iter().enumerate() {
            let actual_index = self.scroll_offset + index;
            let prefix = if actual_index == self.selected {
                "> "
            } else {
                "  "
            };
            let content = format!("{prefix}{theme}");
            rendered.push(format_line_with_width(&content, cols));
        }

        while rendered.len() < capacity {
            rendered.push(format_line_with_width("", cols));
        }

        rendered
    }
}

fn format_line_with_width(content: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let truncated: String = content.chars().take(width).collect();
    let padding = width.saturating_sub(truncated.chars().count());
    let mut line = truncated;
    line.push_str(&" ".repeat(padding));
    line
}

impl ThemeSelectorPlugin {
    fn format_line(&self, content: &str, width: usize) -> String {
        format_line_with_width(content, width)
    }
}
