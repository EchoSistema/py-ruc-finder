use std::sync::Arc;

use actix_web::{App, HttpServer, web};
use clap::Parser;
use log::info;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use ruc_finder::config;
use ruc_finder::db;
use ruc_finder::exporter::ExportFormat;
use ruc_finder::footprint::{FootprintMiddleware, FootprintService};
use ruc_finder::handlers;
use ruc_finder::openapi::ApiDoc;
use ruc_finder::scraper;

#[derive(Parser)]
#[command(name = "ruc_finder", about = "RUC Finder - DNIT Paraguay")]
struct Cli {
    /// Run the scraper sync and exit
    #[arg(long)]
    sync: bool,

    /// Force sync: bypass date, hash, and interval checks
    #[arg(long)]
    force: bool,

    /// Export format when no database is configured: csv, json, neon, parquet
    #[arg(long)]
    format: Option<String>,

    /// Output directory for file exports (overrides config file)
    #[arg(long)]
    output: Option<String>,

    /// Path to config file (default: /etc/ruc_finder/ruc_finder.conf)
    #[arg(long, short)]
    config: Option<String>,

    /// Host/IP to bind the server (overrides config file and env)
    #[arg(long)]
    host: Option<String>,

    /// Port to bind the server (overrides config file and env)
    #[arg(long)]
    port: Option<u16>,

    /// Download files from DB file_url and backfill file_hash for rows that have none
    #[arg(long)]
    backfill_hashes: bool,
}

#[tokio::main]
async fn main() {
    let _ = dotenvy::dotenv();
    env_logger::init();
    let cli = Cli::parse();
    let mut config = config::AppConfig::load(cli.config.as_deref());

    if let Some(host) = &cli.host {
        config.host = host.clone();
    }
    if let Some(port) = cli.port {
        config.port = port;
    }

    if cli.backfill_hashes {
        if !config.has_database() {
            eprintln!("DATABASE_URL is not set. --backfill-hashes requires a database connection.");
            std::process::exit(1);
        }
        let pool = db::create_pool(&config).await;
        info!("Backfilling file hashes from DB file URLs...");
        scraper::backfill_file_hashes(&pool, &config).await;
        return;
    }

    if cli.sync {
        if config.has_database() && cli.format.is_none() {
            if cli.force {
                info!("Running FORCED sync to database...");
            } else {
                info!("Running sync to database...");
            }
            let pool = db::create_pool(&config).await;
            scraper::run_sync_db(&pool, &config, cli.force).await;
        } else {
            let format_str = cli.format.as_deref().unwrap_or("json");
            let format = ExportFormat::from_str(format_str).unwrap_or_else(|| {
                eprintln!(
                    "Unknown format '{}'. Valid options: csv, json, neon, parquet",
                    format_str
                );
                std::process::exit(1);
            });
            info!("Running sync to file ({format_str})...");
            let output_dir = cli.output.as_deref().unwrap_or(&config.output_dir);
            scraper::run_sync_file(format, output_dir, &config, cli.force).await;
        }
        return;
    }

    if !config.has_database() {
        eprintln!("DATABASE_URL is not set. The API server requires a database connection.");
        eprintln!("Use --sync --format <csv|json|neon|parquet> for offline mode.");
        std::process::exit(1);
    }

    let pool = db::create_pool(&config).await;
    let bind_addr = format!("{}:{}", config.host, config.port);
    info!("Starting server on {bind_addr}...");
    let pool_data = web::Data::new(pool);

    // Bootstrap footprint tracking (auto-enabled when both URL and key are set)
    let footprint_svc: Option<Arc<FootprintService>> =
        match (&config.footprint_api_base_url, &config.footprint_public_key) {
            (Some(base_url), Some(key)) => {
                info!("Footprint tracking enabled → {base_url}");
                Some(Arc::new(FootprintService::new(
                    base_url.clone(),
                    key.clone(),
                )))
            }
            _ => None,
        };

    let config_data = web::Data::new(Arc::new(config));
    let openapi = ApiDoc::openapi();
    HttpServer::new(move || {
        App::new()
            .app_data(pool_data.clone())
            .app_data(config_data.clone())
            .wrap(FootprintMiddleware { service: footprint_svc.clone() })
            .service(
                SwaggerUi::new("/swagger-ui/{_:.*}").url("/api-docs/openapi.json", openapi.clone()),
            )
            .route("/swagger-ui", web::get().to(|| async {
                actix_web::HttpResponse::MovedPermanently()
                    .append_header(("Location", "/swagger-ui/"))
                    .finish()
            }))
            .route("/api/v1/health", web::get().to(handlers::health_check))
            .route(
                "/api/v1/ruc/search",
                web::get().to(handlers::fuzzy_search_ruc),
            )
            .route(
                "/api/v1/ruc/{ruc}/dv",
                web::get().to(handlers::compute_check_digit),
            )
            .route(
                "/api/v1/ruc/{ruc_dv}/validate",
                web::get().to(handlers::validate_ruc),
            )
            .route(
                "/api/v1/ruc/{ruc}/validate/{dv}",
                web::get().to(handlers::validate_ruc_split),
            )
            .route(
                "/api/v1/ruc/{ruc}",
                web::get().to(handlers::get_ruc_by_number),
            )
            .route("/api/v1/ruc", web::get().to(handlers::search_ruc))
            .route("/api/v1/sync", web::post().to(handlers::trigger_sync))
    })
    .bind(&bind_addr)
    .expect("Failed to bind server")
    .run()
    .await
    .expect("Server error");
}
