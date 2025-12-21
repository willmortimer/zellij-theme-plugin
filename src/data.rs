use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::io;
use std::env;
use reqwest;
use std::time::{Duration, SystemTime};
use serde_json::Value;
use kdl::{KdlDocument, KdlNode};

const GITHUB_API_URL: &str = "https://api.github.com/repos/zellij-org/zellij/contents/zellij-utils/assets/themes";
const CACHE_DURATION: Duration = Duration::from_secs(3600); // 1 hour

pub struct ThemeData {
    config_path: PathBuf,
    theme_dir: PathBuf,
    cache_path: PathBuf,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct CacheData {
    themes: Vec<String>,
    timestamp: u64,
}

impl ThemeData {
    pub fn new() -> io::Result<Self> {
        let config_path = Self::get_config_path()?;
        let theme_dir = config_path.parent().unwrap().join("themes");
        let cache_path = config_path.parent().unwrap().join(".theme_cache.json");

        Ok(Self {
            config_path,
            theme_dir,
            cache_path,
        })
    }

    fn get_config_path() -> io::Result<PathBuf> {
        if let Ok(dir) = env::var("ZELLIJ_CONFIG_DIR") {
            Ok(PathBuf::from(dir).join("config.kdl"))
        } else {
            let home = env::var("HOME").expect("HOME environment variable not set");
            Ok(PathBuf::from(home).join(".config/zellij/config.kdl"))
        }
    }

    fn read_cache(&self) -> Option<CacheData> {
        if let Ok(content) = fs::read_to_string(&self.cache_path) {
            if let Ok(cache) = serde_json::from_str::<CacheData>(&content) {
                let now = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();

                if now - cache.timestamp < CACHE_DURATION.as_secs() {
                    return Some(cache);
                }
            }
        }
        None
    }

    fn write_cache(&self, themes: &[String]) -> io::Result<()> {
        let cache = CacheData {
            themes: themes.to_vec(),
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        let content = serde_json::to_string(&cache)?;
        fs::write(&self.cache_path, content)?;
        Ok(())
    }

    fn extract_themes_from_kdl(content: &str) -> Vec<String> {
        if let Ok(doc) = content.parse::<KdlDocument>() {
            // Look for the themes node
            if let Some(themes_node) = doc.get("themes") {
                // Get the children of the themes node
                if let Some(children) = themes_node.children() {
                    // Each direct child node of the themes node is a theme
                    return children
                        .nodes()
                        .iter()
                        .map(|node| node.name().to_string())
                        .collect();
                }
            }
        }
        Vec::new()
    }

    /// Load themes from local config.kdl file
    fn get_local_config_themes(&self) -> Vec<String> {
        if let Ok(content) = fs::read_to_string(&self.config_path) {
            return Self::extract_themes_from_kdl(&content);
        }
        Vec::new()
    }

    /// Load themes from local themes directory
    fn get_local_dir_themes(&self) -> Vec<String> {
        let mut themes = Vec::new();
        if self.theme_dir.exists() {
            if let Ok(entries) = fs::read_dir(&self.theme_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map_or(false, |ext| ext == "kdl") {
                        if let Ok(content) = fs::read_to_string(&path) {
                            themes.extend(Self::extract_themes_from_kdl(&content));
                        }
                    }
                }
            }
        }
        themes
    }

    pub async fn fetch_themes(force_refresh: bool) -> io::Result<Vec<String>> {
        let instance = Self::new()?;

        // Always get local themes first (these are always fresh)
        let mut local_themes: Vec<String> = Vec::new();
        local_themes.extend(instance.get_local_config_themes());
        local_themes.extend(instance.get_local_dir_themes());

        // Try to read GitHub themes from cache first unless force refresh is requested
        let mut github_themes: Vec<String> = Vec::new();
        if !force_refresh {
            if let Some(cache) = instance.read_cache() {
                github_themes = cache.themes;
            }
        }

        // Fetch from GitHub if cache miss or force refresh
        if github_themes.is_empty() {
            let client = reqwest::Client::new();
            let response = client
                .get(GITHUB_API_URL)
                .header("User-Agent", "zellij-theme-plugin")
                .send()
                .await
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

            let files: Vec<Value> = response
                .json()
                .await
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

            // Process each file
            for file in files {
                if let Some(name) = file["name"].as_str() {
                    if name.ends_with(".kdl") {
                        // Get the raw content URL
                        if let Some(download_url) = file["download_url"].as_str() {
                            // Download and parse the KDL file
                            if let Ok(content) = client.get(download_url).send().await {
                                if let Ok(text) = content.text().await {
                                    // Parse the KDL file and extract theme names
                                    github_themes.extend(Self::extract_themes_from_kdl(&text));
                                }
                            }
                        }
                    }
                }
            }

            // Cache GitHub themes only
            github_themes.push("default".to_string());
            instance.write_cache(&github_themes)?;
        }

        // Merge local and GitHub themes, removing duplicates
        let mut all_themes: Vec<String> = local_themes;
        for theme in github_themes {
            if !all_themes.contains(&theme) {
                all_themes.push(theme);
            }
        }

        // Sort with local themes first
        all_themes.sort_by(|a, b| {
            let a_local = instance.is_local_theme(a);
            let b_local = instance.is_local_theme(b);
            match (a_local, b_local) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.cmp(b),
            }
        });

        Ok(all_themes)
    }

    pub async fn fetch_themes_with_local_info(force_refresh: bool) -> io::Result<(Vec<String>, HashSet<String>)> {
        let instance = Self::new()?;

        // Get local themes first
        let mut local_themes: Vec<String> = Vec::new();
        local_themes.extend(instance.get_local_config_themes());
        local_themes.extend(instance.get_local_dir_themes());
        let local_set: HashSet<String> = local_themes.iter().cloned().collect();

        // Get all themes using existing method
        let all_themes = Self::fetch_themes(force_refresh).await?;

        Ok((all_themes, local_set))
    }

    fn is_local_theme(&self, theme_name: &str) -> bool {
        // Check if theme exists in config.kdl
        if let Ok(content) = fs::read_to_string(&self.config_path) {
            let local_config_themes = Self::extract_themes_from_kdl(&content);
            if local_config_themes.contains(&theme_name.to_string()) {
                return true;
            }
        }
        // Check if theme exists in themes directory
        if self.theme_dir.exists() {
            if let Ok(entries) = fs::read_dir(&self.theme_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map_or(false, |ext| ext == "kdl") {
                        if let Ok(content) = fs::read_to_string(&path) {
                            let dir_themes = Self::extract_themes_from_kdl(&content);
                            if dir_themes.contains(&theme_name.to_string()) {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }

    pub fn ensure_theme_dir(&self) -> io::Result<()> {
        if !self.theme_dir.exists() {
            fs::create_dir_all(&self.theme_dir)?;
            println!("Created theme directory at: {}", self.theme_dir.display());
        }
        Ok(())
    }

    pub fn update_config(&self, selected_theme: &str) -> io::Result<()> {
        let content = fs::read_to_string(&self.config_path)?;
        let mut doc: KdlDocument = content.parse().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        // Update or add theme node
        if let Some(theme_node) = doc.get_mut("theme") {
            // Clear existing values and entries
            theme_node.clear_entries();
            // Add the new theme value
            theme_node.push(selected_theme);
        } else {
            // Create a new theme node with the value
            let mut node = KdlNode::new("theme");
            node.push(selected_theme);
            doc.nodes_mut().push(node);
        }

        // Write updated document back to file
        fs::write(&self.config_path, doc.to_string())?;
        Ok(())
    }
}
