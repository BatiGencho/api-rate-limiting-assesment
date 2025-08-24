use anyhow::Result;

#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
    pub database_url: String,
    pub redis_url: String,
    pub environment: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            port: std::env::var("PORT")
                .unwrap_or_else(|_| "3000".to_string())
                .parse()?,
            database_url: std::env::var("DATABASE_URL").expect("DATABASE_URL must be set"),
            redis_url: std::env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://localhost:6379".to_string()),
            environment: std::env::var("ENVIRONMENT").unwrap_or_else(|_| "development".to_string()),
        })
    }
}
