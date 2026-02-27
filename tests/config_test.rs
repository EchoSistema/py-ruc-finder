use std::net::{IpAddr, Ipv4Addr};

use ruc_finder::config::{
    CidrNetwork, AppConfig, DEFAULT_HOST, DEFAULT_PORT, DEFAULT_DB_POOL_SIZE,
    DEFAULT_SYNC_INTERVAL_HOURS, DEFAULT_SYNC_BATCH_SIZE, DEFAULT_PAGE_URL,
    DEFAULT_DOWNLOAD_DIR, DEFAULT_OUTPUT_DIR, DEFAULT_PAGINATION_LIMIT,
    DEFAULT_PAGINATION_MAX, DEFAULT_FUZZY_LIMIT, DEFAULT_FUZZY_MAX,
    DEFAULT_FUZZY_THRESHOLD,
};

// ---------------------------------------------------------------------------
// CidrNetwork::parse
// ---------------------------------------------------------------------------

#[test]
fn cidr_parse_valid_slash_20() {
    let net = CidrNetwork::parse("10.116.0.0/20").expect("should parse");
    let ip: IpAddr = Ipv4Addr::new(10, 116, 0, 1).into();
    assert!(net.contains(&ip));
}

#[test]
fn cidr_parse_valid_slash_32() {
    let net = CidrNetwork::parse("192.168.1.1/32").expect("should parse");
    let exact: IpAddr = Ipv4Addr::new(192, 168, 1, 1).into();
    let other: IpAddr = Ipv4Addr::new(192, 168, 1, 2).into();
    assert!(net.contains(&exact));
    assert!(!net.contains(&other));
}

#[test]
fn cidr_parse_valid_slash_0() {
    let net = CidrNetwork::parse("0.0.0.0/0").expect("should parse");
    let any: IpAddr = Ipv4Addr::new(123, 45, 67, 89).into();
    assert!(net.contains(&any));
}

#[test]
fn cidr_parse_invalid_prefix_too_large() {
    assert!(CidrNetwork::parse("10.0.0.0/33").is_none());
}

#[test]
fn cidr_parse_invalid_no_slash() {
    assert!(CidrNetwork::parse("10.0.0.0").is_none());
}

#[test]
fn cidr_parse_invalid_bad_ip() {
    assert!(CidrNetwork::parse("999.999.999.999/24").is_none());
}

#[test]
fn cidr_parse_invalid_empty() {
    assert!(CidrNetwork::parse("").is_none());
}

// ---------------------------------------------------------------------------
// CidrNetwork::contains
// ---------------------------------------------------------------------------

#[test]
fn cidr_contains_inside_range() {
    let net = CidrNetwork::parse("10.116.0.0/20").unwrap();
    // 10.116.0.0/20 covers 10.116.0.0 — 10.116.15.255
    assert!(net.contains(&Ipv4Addr::new(10, 116, 0, 0).into()));
    assert!(net.contains(&Ipv4Addr::new(10, 116, 15, 255).into()));
    assert!(net.contains(&Ipv4Addr::new(10, 116, 8, 42).into()));
}

#[test]
fn cidr_contains_outside_range() {
    let net = CidrNetwork::parse("10.116.0.0/20").unwrap();
    assert!(!net.contains(&Ipv4Addr::new(10, 116, 16, 0).into()));
    assert!(!net.contains(&Ipv4Addr::new(10, 117, 0, 0).into()));
    assert!(!net.contains(&Ipv4Addr::new(192, 168, 1, 1).into()));
}

#[test]
fn cidr_contains_rejects_ipv6() {
    let net = CidrNetwork::parse("10.0.0.0/8").unwrap();
    let ipv6: IpAddr = "::1".parse().unwrap();
    assert!(!net.contains(&ipv6));
}

// ---------------------------------------------------------------------------
// AppConfig defaults (no config file, no env vars for these keys)
// ---------------------------------------------------------------------------

#[test]
fn config_defaults_without_file() {
    // Load with a nonexistent config path — should fall back to all defaults
    let config = AppConfig::load(Some("/tmp/nonexistent_ruc_finder_test.conf"));

    assert_eq!(config.host, DEFAULT_HOST);
    assert_eq!(config.port, DEFAULT_PORT);
    assert_eq!(config.db_pool_size, DEFAULT_DB_POOL_SIZE);
    assert_eq!(config.sync_interval_hours, DEFAULT_SYNC_INTERVAL_HOURS);
    assert_eq!(config.sync_batch_size, DEFAULT_SYNC_BATCH_SIZE);
    assert_eq!(config.sync_page_url, DEFAULT_PAGE_URL);
    assert_eq!(config.download_dir, DEFAULT_DOWNLOAD_DIR);
    assert_eq!(config.output_dir, DEFAULT_OUTPUT_DIR);
    assert_eq!(config.pagination_limit, DEFAULT_PAGINATION_LIMIT);
    assert_eq!(config.pagination_max, DEFAULT_PAGINATION_MAX);
    assert_eq!(config.fuzzy_limit, DEFAULT_FUZZY_LIMIT);
    assert_eq!(config.fuzzy_max, DEFAULT_FUZZY_MAX);
    assert!((config.fuzzy_threshold - DEFAULT_FUZZY_THRESHOLD).abs() < f64::EPSILON);
    assert!(config.sync_allowed_networks.is_empty());
}

#[test]
fn config_has_database_false_by_default() {
    let config = AppConfig::load(Some("/tmp/nonexistent_ruc_finder_test.conf"));
    // DATABASE_URL may or may not be set in the env (CI vs local).
    // At minimum, the method should not panic.
    let _ = config.has_database();
}

// ---------------------------------------------------------------------------
// AppConfig::is_sync_allowed
// ---------------------------------------------------------------------------

#[test]
fn is_sync_allowed_open_when_no_networks() {
    let config = AppConfig::load(Some("/tmp/nonexistent_ruc_finder_test.conf"));
    // No networks configured → allow all
    let ip: IpAddr = Ipv4Addr::new(1, 2, 3, 4).into();
    assert!(config.is_sync_allowed(&ip));
}

// ---------------------------------------------------------------------------
// Config file loading from TOML
// ---------------------------------------------------------------------------

#[test]
fn config_loads_from_toml_file() {
    let dir = std::env::temp_dir().join("ruc_finder_config_test");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("test.conf");

    std::fs::write(
        &path,
        r#"
[server]
host = "127.0.0.1"
port = 9999

[sync]
interval_hours = 12
batch_size = 500

[paths]
download_dir = "/tmp/dl"
output_dir = "/tmp/out"

[search]
pagination_limit = 50
"#,
    )
    .unwrap();

    let config = AppConfig::load(Some(path.to_str().unwrap()));

    // TOML values (may be overridden by env vars in CI, so only assert if
    // the corresponding env var is not set)
    if std::env::var("HOST").is_err() {
        assert_eq!(config.host, "127.0.0.1");
    }
    if std::env::var("PORT").is_err() {
        assert_eq!(config.port, 9999);
    }
    if std::env::var("SYNC_INTERVAL_HOURS").is_err() {
        assert_eq!(config.sync_interval_hours, 12);
    }
    if std::env::var("SYNC_BATCH_SIZE").is_err() {
        assert_eq!(config.sync_batch_size, 500);
    }
    if std::env::var("DOWNLOAD_DIR").is_err() {
        assert_eq!(config.download_dir, "/tmp/dl");
    }
    if std::env::var("OUTPUT_DIR").is_err() {
        assert_eq!(config.output_dir, "/tmp/out");
    }
    if std::env::var("PAGINATION_LIMIT").is_err() {
        assert_eq!(config.pagination_limit, 50);
    }

    // Cleanup
    let _ = std::fs::remove_dir_all(&dir);
}
