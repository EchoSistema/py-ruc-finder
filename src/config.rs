use std::env;
use std::net::{IpAddr, Ipv4Addr};
use std::path::Path;

use log::info;
use serde::Deserialize;

pub const DEFAULT_CONFIG_PATH: &str = "/etc/ruc_finder/ruc_finder.conf";
pub const DEFAULT_HOST: &str = "0.0.0.0";
pub const DEFAULT_PORT: u16 = 3000;
pub const DEFAULT_DB_POOL_SIZE: u32 = 10;
pub const DEFAULT_SYNC_INTERVAL_HOURS: u64 = 24;
pub const DEFAULT_SYNC_BATCH_SIZE: usize = 1000;
pub const DEFAULT_PAGE_URL: &str =
    "https://www.dnit.gov.py/web/portal-institucional/listado-de-ruc-con-sus-equivalencias";
pub const DEFAULT_DOWNLOAD_DIR: &str = "input/tmp";
pub const DEFAULT_OUTPUT_DIR: &str = "./output";
pub const DEFAULT_PAGINATION_LIMIT: i64 = 25;
pub const DEFAULT_PAGINATION_MAX: i64 = 200;
pub const DEFAULT_FUZZY_LIMIT: i64 = 25;
pub const DEFAULT_FUZZY_MAX: i64 = 200;
pub const DEFAULT_FUZZY_THRESHOLD: f64 = 0.3;
pub const DEFAULT_FUZZY_THRESHOLD_MIN: f64 = 0.1;
pub const DEFAULT_FUZZY_THRESHOLD_MAX: f64 = 0.9;

/// TOML file structure — all fields optional so partial configs work.
#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    #[serde(default)]
    server: ServerSection,
    #[serde(default)]
    database: DatabaseSection,
    #[serde(default)]
    sync: SyncSection,
    #[serde(default)]
    paths: PathsSection,
    #[serde(default)]
    search: SearchSection,
}

#[derive(Debug, Default, Deserialize)]
struct ServerSection {
    host: Option<String>,
    port: Option<u16>,
}

#[derive(Debug, Default, Deserialize)]
struct DatabaseSection {
    url: Option<String>,
    pool_size: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
struct SyncSection {
    interval_hours: Option<u64>,
    batch_size: Option<usize>,
    page_url: Option<String>,
    allowed_networks: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
struct PathsSection {
    download_dir: Option<String>,
    output_dir: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct SearchSection {
    pagination_limit: Option<i64>,
    pagination_max: Option<i64>,
    fuzzy_limit: Option<i64>,
    fuzzy_max: Option<i64>,
    fuzzy_threshold: Option<f64>,
    fuzzy_threshold_min: Option<f64>,
    fuzzy_threshold_max: Option<f64>,
}

pub struct AppConfig {
    pub database_url: Option<String>,
    pub db_pool_size: u32,
    pub host: String,
    pub port: u16,
    pub sync_interval_hours: u64,
    pub sync_batch_size: usize,
    pub sync_page_url: String,
    pub download_dir: String,
    pub output_dir: String,
    pub pagination_limit: i64,
    pub pagination_max: i64,
    pub fuzzy_limit: i64,
    pub fuzzy_max: i64,
    pub fuzzy_threshold: f64,
    pub fuzzy_threshold_min: f64,
    pub fuzzy_threshold_max: f64,
    /// CIDR networks allowed to call POST /api/v1/sync.
    /// Empty = no restriction (allow all).
    pub sync_allowed_networks: Vec<CidrNetwork>,
}

/// Parsed CIDR network (e.g. 10.116.0.0/20).
#[derive(Debug, Clone)]
pub struct CidrNetwork {
    network: u32,
    mask: u32,
}

impl CidrNetwork {
    /// Parse a CIDR string like "10.116.0.0/20". Returns None if invalid.
    pub fn parse(cidr: &str) -> Option<Self> {
        let (addr_str, prefix_str) = cidr.split_once('/')?;
        let addr: Ipv4Addr = addr_str.parse().ok()?;
        let prefix: u32 = prefix_str.parse().ok()?;
        if prefix > 32 {
            return None;
        }
        let mask = if prefix == 0 {
            0
        } else {
            !0u32 << (32 - prefix)
        };
        let network = u32::from(addr) & mask;
        Some(Self { network, mask })
    }

    /// Check if an IP address belongs to this CIDR network.
    pub fn contains(&self, ip: &IpAddr) -> bool {
        match ip {
            IpAddr::V4(v4) => (u32::from(*v4) & self.mask) == self.network,
            IpAddr::V6(_) => false,
        }
    }
}

impl AppConfig {
    /// Load config with precedence: env vars > config file > defaults.
    pub fn load(config_path: Option<&str>) -> Self {
        let file = Self::read_config_file(config_path);

        Self {
            database_url: env::var("DATABASE_URL").ok().or(file.database.url),
            db_pool_size: env::var("DB_POOL_SIZE")
                .ok()
                .and_then(|v| v.parse().ok())
                .or(file.database.pool_size)
                .unwrap_or(DEFAULT_DB_POOL_SIZE),
            host: env::var("HOST")
                .ok()
                .or(file.server.host)
                .unwrap_or_else(|| DEFAULT_HOST.to_string()),
            port: env::var("PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .or(file.server.port)
                .unwrap_or(DEFAULT_PORT),
            sync_interval_hours: env::var("SYNC_INTERVAL_HOURS")
                .ok()
                .and_then(|v| v.parse().ok())
                .or(file.sync.interval_hours)
                .unwrap_or(DEFAULT_SYNC_INTERVAL_HOURS),
            sync_batch_size: env::var("SYNC_BATCH_SIZE")
                .ok()
                .and_then(|v| v.parse().ok())
                .or(file.sync.batch_size)
                .unwrap_or(DEFAULT_SYNC_BATCH_SIZE),
            sync_page_url: env::var("SYNC_PAGE_URL")
                .ok()
                .or(file.sync.page_url)
                .unwrap_or_else(|| DEFAULT_PAGE_URL.to_string()),
            download_dir: env::var("DOWNLOAD_DIR")
                .ok()
                .or(file.paths.download_dir)
                .unwrap_or_else(|| DEFAULT_DOWNLOAD_DIR.to_string()),
            output_dir: env::var("OUTPUT_DIR")
                .ok()
                .or(file.paths.output_dir)
                .unwrap_or_else(|| DEFAULT_OUTPUT_DIR.to_string()),
            pagination_limit: env::var("PAGINATION_LIMIT")
                .ok()
                .and_then(|v| v.parse().ok())
                .or(file.search.pagination_limit)
                .unwrap_or(DEFAULT_PAGINATION_LIMIT),
            pagination_max: env::var("PAGINATION_MAX")
                .ok()
                .and_then(|v| v.parse().ok())
                .or(file.search.pagination_max)
                .unwrap_or(DEFAULT_PAGINATION_MAX),
            fuzzy_limit: env::var("FUZZY_LIMIT")
                .ok()
                .and_then(|v| v.parse().ok())
                .or(file.search.fuzzy_limit)
                .unwrap_or(DEFAULT_FUZZY_LIMIT),
            fuzzy_max: env::var("FUZZY_MAX")
                .ok()
                .and_then(|v| v.parse().ok())
                .or(file.search.fuzzy_max)
                .unwrap_or(DEFAULT_FUZZY_MAX),
            fuzzy_threshold: env::var("FUZZY_THRESHOLD")
                .ok()
                .and_then(|v| v.parse().ok())
                .or(file.search.fuzzy_threshold)
                .unwrap_or(DEFAULT_FUZZY_THRESHOLD),
            fuzzy_threshold_min: env::var("FUZZY_THRESHOLD_MIN")
                .ok()
                .and_then(|v| v.parse().ok())
                .or(file.search.fuzzy_threshold_min)
                .unwrap_or(DEFAULT_FUZZY_THRESHOLD_MIN),
            fuzzy_threshold_max: env::var("FUZZY_THRESHOLD_MAX")
                .ok()
                .and_then(|v| v.parse().ok())
                .or(file.search.fuzzy_threshold_max)
                .unwrap_or(DEFAULT_FUZZY_THRESHOLD_MAX),
            sync_allowed_networks: Self::parse_allowed_networks(
                env::var("SYNC_ALLOWED_NETWORKS").ok().as_deref(),
                file.sync.allowed_networks.as_deref(),
            ),
        }
    }

    fn read_config_file(config_path: Option<&str>) -> FileConfig {
        let path = config_path.unwrap_or(DEFAULT_CONFIG_PATH);
        let path = Path::new(path);

        if !path.exists() {
            return FileConfig::default();
        }

        match std::fs::read_to_string(path) {
            Ok(contents) => match toml::from_str::<FileConfig>(&contents) {
                Ok(cfg) => {
                    info!("Loaded config from {}", path.display());
                    cfg
                }
                Err(e) => {
                    log::warn!("Failed to parse {}: {e}", path.display());
                    FileConfig::default()
                }
            },
            Err(e) => {
                log::warn!("Failed to read {}: {e}", path.display());
                FileConfig::default()
            }
        }
    }

    /// Parse CIDR networks from env var (comma-separated) or config file (TOML array).
    fn parse_allowed_networks(
        env_val: Option<&str>,
        file_val: Option<&[String]>,
    ) -> Vec<CidrNetwork> {
        let raw: Vec<String> = if let Some(env) = env_val {
            env.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        } else if let Some(file) = file_val {
            file.to_vec()
        } else {
            return Vec::new();
        };

        raw.iter()
            .filter_map(|cidr| {
                CidrNetwork::parse(cidr).or_else(|| {
                    log::warn!("Invalid CIDR in sync.allowed_networks: {cidr}");
                    None
                })
            })
            .collect()
    }

    pub fn has_database(&self) -> bool {
        self.database_url.is_some()
    }

    /// Check if an IP is allowed to call the sync endpoint.
    /// Returns true if no networks are configured (open) or if the IP matches at least one.
    pub fn is_sync_allowed(&self, ip: &IpAddr) -> bool {
        if self.sync_allowed_networks.is_empty() {
            return true;
        }
        self.sync_allowed_networks
            .iter()
            .any(|net| net.contains(ip))
    }
}
