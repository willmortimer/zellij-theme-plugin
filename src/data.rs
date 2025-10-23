use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use kdl::{KdlDocument, KdlNode};
use serde::Deserialize;

pub const GITHUB_API_URL: &str =
    "https://api.github.com/repos/zellij-org/zellij/contents/zellij-utils/assets/themes";
pub const CACHE_DURATION: Duration = Duration::from_secs(3600); // 1 hour

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

#[derive(Debug, Clone, Deserialize)]
pub struct GithubFile {
    pub name: String,
    #[serde(default)]
    pub download_url: Option<String>,
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

    pub fn read_cache(&self) -> io::Result<Option<Vec<String>>> {
        match fs::read_to_string(&self.cache_path) {
            Ok(content) => match serde_json::from_str::<CacheData>(&content) {
                Ok(cache) => {
                    let now = SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_secs();

                    if now - cache.timestamp < CACHE_DURATION.as_secs() {
                        Ok(Some(cache.themes))
                    } else {
                        Ok(None)
                    }
                }
                Err(err) => Err(io::Error::new(io::ErrorKind::Other, err)),
            },
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }

    pub fn write_cache(&self, themes: &[String]) -> io::Result<()> {
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

    pub fn extract_themes_from_kdl(content: &str) -> Vec<String> {
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

    pub fn ensure_theme_dir(&self) -> io::Result<()> {
        if !self.theme_dir.exists() {
            fs::create_dir_all(&self.theme_dir)?;
            println!("Created theme directory at: {}", self.theme_dir.display());
        }
        Ok(())
    }

    pub fn update_config(&self, selected_theme: &str) -> io::Result<()> {
        let content = fs::read_to_string(&self.config_path)?;
        let mut doc: KdlDocument = content
            .parse()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

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
