use dirs::home_dir;
use std::path::PathBuf;

pub struct Paths {
    pub aws_dir: PathBuf,
    pub config: PathBuf,
    pub credentials: PathBuf,
    pub chromium: PathBuf,
}

impl Paths {
    pub fn new() -> Self {
        let aws_dir = home_dir()
            .expect("Could not determine home directory")
            .join(".aws");

        let config = std::env::var("AWS_CONFIG_FILE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| aws_dir.join("config"));

        let credentials = std::env::var("AWS_SHARED_CREDENTIALS_FILE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| aws_dir.join("credentials"));

        let chromium = aws_dir.join("chromium");

        Paths {
            aws_dir,
            config,
            credentials,
            chromium,
        }
    }
}

impl Default for Paths {
    fn default() -> Self {
        Self::new()
    }
}
